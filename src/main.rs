mod recent_lines;
mod samples;
mod tui;
mod watcher;

use tui::*;
use watcher::watch_path;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample, SupportedStreamConfig};
use glicol::Engine;
use std::error::Error;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::{self, Arc};
use std::time::{Duration, Instant}; // , SystemTime, UNIX_EPOCH
use std::{io, thread}; // use std::time::{Instant};
use tracing::error;
use tracing_subscriber::fmt::{format::FmtSpan, time::ChronoLocal};

pub const RB_SIZE: usize = 200;
pub const BLOCK_SIZE: usize = 128;

/// Glicol cli tool. This tool will watch the changes in a .glicol file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the .glicol file
    #[arg(index = 1)]
    file: String,

    // Show a scope or not
    // #[arg(short, long)]
    // scope: bool,
    /// Set beats per minute (BPM)
    #[arg(short, long, default_value_t = 120.0)]
    bpm: f32,

    /// The audio device to use
    #[arg(short, long, default_value_t = String::from("default"))]
    device: String,

    /// Use the JACK host
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    #[arg(short, long)]
    #[allow(dead_code)]
    jack: bool,

    /// Disable the TUI
    #[arg(short = 'H', long, action = clap::ArgAction::SetTrue)]
    headless: bool,
}

#[allow(unused_must_use)]
fn main() -> Result<(), Box<dyn Error>> {
    // Print help screen if no args provided:
    if std::env::args().len() == 1 {
        Args::command().print_help()?;
        println!();
        return Ok(());
    }
    let args = Args::parse();
    let path = args.file;
    // let scope = args.scope;
    let device = args.device;
    let bpm = args.bpm;

    // keep logs
    const RECENT_LINES_COUNT: usize = 100;

    // let mut ringbuf_l = [0.0; RB_SIZE];
    // let mut ringbuf_r = [0.0; RB_SIZE];
    // let index = Arc::new(AtomicUsize::new(0));
    // let index_clone = Arc::clone(&index);
    // let ptr_rb_left = Arc::new(AtomicPtr::<f32>::new(ringbuf_l.as_mut_ptr()));
    // let ptr_rb_right = Arc::new(AtomicPtr::<f32>::new(ringbuf_r.as_mut_ptr()));
    // let ptr_rb_left_clone = Arc::clone(&ptr_rb_left);
    // let ptr_rb_right_clone = Arc::clone(&ptr_rb_right);

    let mut samples_l = [0.0; RB_SIZE];
    let mut samples_r = [0.0; RB_SIZE];

    let sample_data = Arc::new(SampleData {
        left_ptr: AtomicPtr::<f32>::new(samples_l.as_mut_ptr()),
        right_ptr: AtomicPtr::<f32>::new(samples_r.as_mut_ptr()),
        index: AtomicUsize::new(0),
        capacity: AtomicU32::new(0),
        paused: AtomicBool::new(false),
    });
    // let is_stopping = Arc::new(AtomicBool::new(false));
    // let is_stopping_clone = Arc::clone(&is_stopping);

    // Conditionally compile with jack if the feature is specified.
    #[cfg(all(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        ),
        feature = "jack"
    ))]
    let host = if args.jack {
        cpal::host_from_id(cpal::available_hosts()
            .into_iter()
            .find(|id| *id == cpal::HostId::Jack)
            .expect(
                "make sure --features jack is specified. only works on OSes where jack is available",
            )).expect("jack host unavailable")
    } else {
        cpal::default_host()
    };

    #[cfg(any(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd"
        )),
        not(feature = "jack")
    ))]
    let host = cpal::default_host();

    let device = if device == "default" {
        host.default_output_device()
            .expect("No default output device found")
    } else {
        let Some(device) = host
            .output_devices()?
            .find(|x| x.name().map_or(false, |y| y == device))
        else {
            eprintln!("Couldn't find output device '{device}'. Available options are:");
            for dev_name in host.output_devices()?.filter_map(|d| d.name().ok()) {
                eprintln!("  {dev_name}");
            }
            std::process::exit(1);
        };

        device
    };

    // println!("Output device: {}", device.name()?);
    let config = device.default_output_config()?;

    // limit to stereo
    let config = SupportedStreamConfig::new(
        2,
        config.sample_rate(),
        config.buffer_size().clone(),
        config.sample_format(),
    );
    // println!("Default output config: {:?}", config);

    let info: String = format!("{:?} {:?}", device.name()?.clone(), config.clone());

    // get file updates, keep watching until the end
    let (_watcher, code_updates) = watch_path(Path::new(&path)).context("watch path")?;

    let sample_data_clone = sample_data.clone();
    let audio_thread = thread::spawn(move || {
        if let Err(e) = match config.sample_format() {
            cpal::SampleFormat::I8 => run_audio::<i8>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            cpal::SampleFormat::I16 => run_audio::<i16>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            // cpal::SampleFormat::I24 => run::<I24>(&device, &config.into()),
            cpal::SampleFormat::I32 => run_audio::<i32>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            // cpal::SampleFormat::I48 => run::<I48>(&device, &config.into()),
            cpal::SampleFormat::I64 => run_audio::<i64>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            cpal::SampleFormat::U8 => run_audio::<u8>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            cpal::SampleFormat::U16 => run_audio::<u16>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            // cpal::SampleFormat::U24 => run::<U24>(&device, &config.into()),
            cpal::SampleFormat::U32 => run_audio::<u32>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            // cpal::SampleFormat::U48 => run::<U48>(&device, &config.into()),
            cpal::SampleFormat::U64 => run_audio::<u64>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            cpal::SampleFormat::F32 => run_audio::<f32>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            cpal::SampleFormat::F64 => run_audio::<f64>(
                &device,
                &config.into(),
                code_updates,
                bpm,
                sample_data_clone,
            ),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        } {
            error!("run audio: {e:#}")
        }
    });

    match args.headless {
        true => {
            tracing_subscriber::fmt()
                .with_timer(ChronoLocal::new(String::from("%H:%M:%S%.3f")))
                .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .init();
        }
        false => {
            // setup terminal
            let console_buffer = recent_lines::register_tracer(RECENT_LINES_COUNT);
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;

            let tick_rate = Duration::from_millis(16);
            let res = run_app(
                console_buffer,
                &mut terminal,
                tick_rate,
                sample_data,
                // scope,
                info,
            );

            // restore terminal
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            match res {
                Ok(ExitStatus::ExitAll) => std::process::exit(0),
                Ok(ExitStatus::KeepAudio) => (),
                Err(e) => println!("{e:?}"),
            };
        }
    }
    audio_thread.join().unwrap();
    Ok(())
}

struct SampleData {
    left_ptr: AtomicPtr<f32>,
    right_ptr: AtomicPtr<f32>,
    index: AtomicUsize,
    capacity: AtomicU32,
    paused: AtomicBool,
}

fn run_audio<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    code_updates: sync::mpsc::Receiver<String>,
    bpm: f32,
    sample_data: Arc<SampleData>,
) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let sr = config.sample_rate.0 as usize;

    let mut engine = Engine::<BLOCK_SIZE>::new();
    samples::load_samples_from_env(&mut engine);

    engine.set_sr(sr);
    engine.set_bpm(bpm);

    let channels = 2_usize; //config.channels as usize;

    let mut prev_block: [glicol_synth::Buffer<BLOCK_SIZE>; 2] = [glicol_synth::Buffer::SILENT; 2];

    let ptr = prev_block.as_mut_ptr();
    let prev_block_ptr = Arc::new(AtomicPtr::<glicol_synth::Buffer<BLOCK_SIZE>>::new(ptr));
    let prev_block_len = Arc::new(AtomicUsize::new(prev_block.len()));

    let mut prev_block_pos: usize = BLOCK_SIZE;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            match code_updates.try_recv() {
                Ok(code) => engine.update_with_code(&code),
                Err(TryRecvError::Empty) => {} // nothing new
                Err(TryRecvError::Disconnected) => panic!("code updater is gone"), // closing down
            };

            if sample_data.paused.load(Ordering::Relaxed) {
                for d in &mut *data {
                    *d = T::from_sample(0.);
                }
                return;
            }

            let block_step = data.len() / channels;

            let samples_left_ptr = sample_data.left_ptr.load(Ordering::SeqCst);
            let samples_right_ptr = sample_data.right_ptr.load(Ordering::SeqCst);

            let start_time = Instant::now();

            let mut write_samples =
                |block: &[glicol_synth::Buffer<BLOCK_SIZE>], sample_i: usize, i: usize| {
                    for chan in 0..channels {
                        let samples_i = sample_data.index.load(Ordering::SeqCst);
                        unsafe {
                            match chan {
                                0 => samples_left_ptr.add(samples_i).write(block[chan][i]),
                                1 => samples_right_ptr.add(samples_i).write(block[chan][i]),
                                _ => panic!(),
                            };
                        };

                        sample_data
                            .index
                            .store((samples_i + 1) % 200, Ordering::SeqCst);

                        let value: T = T::from_sample(block[chan][i]);
                        data[sample_i * channels + chan] = value;
                    }
                };

            let ptr = prev_block_ptr.load(Ordering::Acquire);
            let len = prev_block_len.load(Ordering::Acquire);
            let prev_block: &mut [glicol_synth::Buffer<BLOCK_SIZE>] =
                unsafe { std::slice::from_raw_parts_mut(ptr, len) };

            let mut writes = 0;

            for i in prev_block_pos..BLOCK_SIZE {
                write_samples(prev_block, writes, i);
                writes += 1;
            }

            prev_block_pos = BLOCK_SIZE;
            while writes < block_step {
                let (block, raw_err) = engine.next_block(vec![]);
                if raw_err[0] != 0 {
                    let raw_msg = Vec::from(&raw_err[1..]);
                    match String::from_utf8(raw_msg) {
                        Ok(msg) => error!("get next block of engine: {msg}"),
                        Err(e) => error!("got error from engine but unable to decode it: {e}"),
                    }
                }

                if writes + BLOCK_SIZE <= block_step {
                    for i in 0..BLOCK_SIZE {
                        write_samples(block, writes, i);
                        writes += 1;
                    }
                } else {
                    let e = block_step - writes;
                    for i in 0..e {
                        write_samples(block, writes, i);
                        writes += 1;
                    }
                    for (buffer, block) in prev_block.iter_mut().zip(block.iter()) {
                        buffer.copy_from_slice(block);
                    }
                    prev_block_pos = e;
                    break;
                }
            }

            let elapsed_time = start_time.elapsed().as_nanos() as f32;
            let allowed_ns = block_step as f32 * 1_000_000_000.0 / sr as f32;
            let perc = elapsed_time / allowed_ns;
            sample_data
                .capacity
                .store(perc.to_bits(), Ordering::Release);

            // rms = rms
            //     .into_iter()
            //     .map(|x| (x / block_step as f32).sqrt())
            //     .collect();
            // left rms[0] right rms[1]

            // let ptr_l = ptr_rb_left_clone.load(Ordering::SeqCst);
            // let ptr_r = ptr_rb_right_clone.load(Ordering::SeqCst);

            // let len = RB_SIZE;
            // let idx = index_clone.load(Ordering::SeqCst);
            // unsafe {
            //     ptr_l.add(idx).write(rms[0]);
            //     ptr_r.add(idx).write(rms[1]);
            // };
            // index_clone.store((idx + 1) % RB_SIZE, Ordering::SeqCst); // from 0, 1, 2, RB_SIZE-1;
        },
        |err| error!("an error occurred on stream: {err}"),
        None,
    )?;
    stream.play()?;

    loop {
        thread::park() // wait forever
    }
}
