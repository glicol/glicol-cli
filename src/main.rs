mod samples;
mod tui;

use tui::*;

use std::error::Error;
use std::fs::{metadata, File};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant}; // , SystemTime, UNIX_EPOCH
use std::{io, thread}; // use std::time::{Instant};

use anyhow::Result;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use glicol::Engine;

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
}

#[allow(unused_must_use)]
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let path = args.file;
    // let scope = args.scope;
    let device = args.device;
    let bpm = args.bpm;

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // let mut ringbuf_l = [0.0; RB_SIZE];
    // let mut ringbuf_r = [0.0; RB_SIZE];
    let mut samples_l = [0.0; RB_SIZE];
    let mut samples_r = [0.0; RB_SIZE];
    let samples_index = Arc::new(AtomicUsize::new(0));
    let samples_index_clone = Arc::clone(&samples_index);
    // let index = Arc::new(AtomicUsize::new(0));
    // let index_clone = Arc::clone(&index);
    // let ptr_rb_left = Arc::new(AtomicPtr::<f32>::new(ringbuf_l.as_mut_ptr()));
    // let ptr_rb_right = Arc::new(AtomicPtr::<f32>::new(ringbuf_r.as_mut_ptr()));
    // let ptr_rb_left_clone = Arc::clone(&ptr_rb_left);
    // let ptr_rb_right_clone = Arc::clone(&ptr_rb_right);

    let samples_l_ptr = Arc::new(AtomicPtr::<f32>::new(samples_l.as_mut_ptr()));
    let samples_r_ptr = Arc::new(AtomicPtr::<f32>::new(samples_r.as_mut_ptr()));
    let samples_l_ptr_clone = Arc::clone(&samples_l_ptr);
    let samples_r_ptr_clone = Arc::clone(&samples_r_ptr);

    // let is_stopping = Arc::new(AtomicBool::new(false));
    // let is_stopping_clone = Arc::clone(&is_stopping);

    let capacity = Arc::new(AtomicU32::new(0));
    let capacity_clone = Arc::clone(&capacity);

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
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == device).unwrap_or(false))
    }
    .expect("failed to find output device");
    // println!("Output device: {}", device.name()?);
    let config = device.default_output_config().unwrap();
    // println!("Default output config: {:?}", config);

    let info: String = format!("{:?} {:?}", device.name()?.clone(), config.clone());

    let audio_thread = thread::spawn(move || {
        let options = (
            samples_l_ptr_clone,
            samples_r_ptr_clone,
            samples_index_clone,
            path,
            bpm,
            capacity_clone,
        );
        match config.sample_format() {
            cpal::SampleFormat::I8 => run_audio::<i8>(&device, &config.into(), options),
            cpal::SampleFormat::I16 => run_audio::<i16>(&device, &config.into(), options),
            // cpal::SampleFormat::I24 => run::<I24>(&device, &config.into()),
            cpal::SampleFormat::I32 => run_audio::<i32>(&device, &config.into(), options),
            // cpal::SampleFormat::I48 => run::<I48>(&device, &config.into()),
            cpal::SampleFormat::I64 => run_audio::<i64>(&device, &config.into(), options),
            cpal::SampleFormat::U8 => run_audio::<u8>(&device, &config.into(), options),
            cpal::SampleFormat::U16 => run_audio::<u16>(&device, &config.into(), options),
            // cpal::SampleFormat::U24 => run::<U24>(&device, &config.into()),
            cpal::SampleFormat::U32 => run_audio::<u32>(&device, &config.into(), options),
            // cpal::SampleFormat::U48 => run::<U48>(&device, &config.into()),
            cpal::SampleFormat::U64 => run_audio::<u64>(&device, &config.into(), options),
            cpal::SampleFormat::F32 => run_audio::<f32>(&device, &config.into(), options),
            cpal::SampleFormat::F64 => run_audio::<f64>(&device, &config.into(), options),
            sample_format => panic!("Unsupported sample format '{sample_format}'"),
        }
    });

    let tick_rate = Duration::from_millis(16);
    let res = run_app(
        &mut terminal,
        tick_rate,
        samples_l_ptr,
        samples_r_ptr,
        samples_index,
        // scope,
        info,
        capacity,
    );

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    if let Err(err) = res {
        println!("{:?}", err)
    }
    audio_thread.join().unwrap();
    Ok(())
}

// , play_audio: Arc<AtomicBool>
pub fn run_audio<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    options: (
        // Arc<AtomicPtr<f32>>,
        // Arc<AtomicPtr<f32>>,
        // Arc<AtomicUsize>,
        Arc<AtomicPtr<f32>>,
        Arc<AtomicPtr<f32>>,
        Arc<AtomicUsize>,
        String,
        f32,
        Arc<AtomicU32>,
    ),
) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{
    // let ptr_rb_left_clone = options.0;
    // let ptr_rb_right_clone = options.1;
    // let index_clone = options.2;
    let samples_l_ptr_clone = options.0;
    let samples_r_ptr_clone = options.1;
    let samples_index_clone = options.2;
    let path = options.3;
    let bpm = options.4;
    let capacity = options.5;

    let mut last_modified_time = metadata(&path)?.modified()?;

    let sr = config.sample_rate.0 as usize;

    let mut engine = Engine::<BLOCK_SIZE>::new();
    samples::load_samples_from_env(&mut engine);

    let mut code = String::new();
    let ptr = unsafe { code.as_bytes_mut().as_mut_ptr() };
    let code_ptr = Arc::new(AtomicPtr::<u8>::new(ptr));
    let code_len = Arc::new(AtomicUsize::new(code.len()));
    let has_update = Arc::new(AtomicBool::new(true));

    let _code_ptr = Arc::clone(&code_ptr);
    let _code_len = Arc::clone(&code_len);
    let _has_update = Arc::clone(&has_update);

    engine.set_sr(sr);
    engine.set_bpm(bpm);

    let channels = 2 as usize; //config.channels as usize;

    let mut prev_block: [glicol_synth::Buffer<BLOCK_SIZE>; 2] = [glicol_synth::Buffer::SILENT; 2];

    let ptr = prev_block.as_mut_ptr();
    let prev_block_ptr = Arc::new(AtomicPtr::<glicol_synth::Buffer<BLOCK_SIZE>>::new(ptr));
    let prev_block_len = Arc::new(AtomicUsize::new(prev_block.len()));

    let mut prev_block_pos: usize = BLOCK_SIZE;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            if _has_update.load(Ordering::Acquire) {
                let ptr = _code_ptr.load(Ordering::Acquire);
                let len = _code_len.load(Ordering::Acquire);
                let encoded: &[u8] = unsafe { std::slice::from_raw_parts(ptr, len) };
                let code = std::str::from_utf8(encoded).unwrap().to_owned();
                engine.update_with_code(&code);
                _has_update.store(false, Ordering::Release);
            };

            let block_step = data.len() / channels;
            let mut rms: Vec<f32> = vec![0.0; channels];

            let samples_left_ptr = samples_l_ptr_clone.load(Ordering::SeqCst);
            let samples_right_ptr = samples_r_ptr_clone.load(Ordering::SeqCst);

            let start_time = Instant::now();

            let mut write_samples =
                |block: &[glicol_synth::Buffer<BLOCK_SIZE>], sample_i: usize, i: usize| {
                    for chan in 0..channels {
                        let samples_i = samples_index_clone.load(Ordering::SeqCst);
                        unsafe {
                            match chan {
                                0 => samples_left_ptr.add(samples_i).write(block[chan][i]),
                                1 => samples_right_ptr.add(samples_i).write(block[chan][i]),
                                _ => panic!(),
                            };
                        };

                        samples_index_clone.store((samples_i + 1) % 200, Ordering::SeqCst);

                        rms[chan] += block[chan][i].powf(2.0);
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
                let (block, _err_msg) = engine.next_block(vec![]);
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
                    let mut i = 0;
                    for buffer in prev_block.iter_mut() {
                        buffer.copy_from_slice(&block[i]);
                        i += 1;
                    }
                    prev_block_pos = e;
                    break;
                }
            }

            let elapsed_time = start_time.elapsed().as_nanos() as f32;
            let allowed_ns = block_step as f32 * 1_000_000_000.0 / sr as f32;
            let perc = elapsed_time / allowed_ns;
            capacity.store(perc.to_bits(), Ordering::Release);

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
        err_fn,
        None,
    )?;
    stream.play()?;

    loop {
        std::thread::sleep(Duration::from_millis(8));
        let modified_time = metadata(&path)?.modified()?;

        if modified_time != last_modified_time || has_update.load(Ordering::SeqCst) {
            last_modified_time = modified_time;
            let file = File::open(&path)?;
            let reader = BufReader::new(file);
            code = "".to_owned();
            for line in reader.lines() {
                code.push_str(&line?);
                code.push_str("\n");
            }
            // let current_time = SystemTime::now();
            // let unix_time = current_time.duration_since(UNIX_EPOCH).expect("Time went backwards");
            // let system_time = UNIX_EPOCH + unix_time;
            // let datetime = DateTime::<Utc>::from(system_time);
            // println!("```");
            // println!("\n// utc time: {} \n", datetime.format("%Y-%m-%d %H:%M:%S").to_string());
            // println!("{}", code);
            // println!("```");
            code_ptr.store(
                unsafe { code.as_bytes_mut().as_mut_ptr() },
                Ordering::SeqCst,
            );
            code_len.store(code.len(), Ordering::SeqCst);
            has_update.store(true, Ordering::SeqCst);
        }
    }
}
