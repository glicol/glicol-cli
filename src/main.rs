use std::fs::{File, metadata};
use std::io::{BufRead, BufReader};
use std::time::{Instant, Duration}; // , SystemTime, UNIX_EPOCH
use std::error::Error;
use std::{io, thread}; // use std::time::{Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, AtomicPtr, Ordering};

use anyhow::Result;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use glicol::Engine;
// use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Sparkline},
    Frame, Terminal,
};

const RB_SIZE: usize = 200;
const BLOCK_SIZE: usize = 128;

/// Glicol cli tool. This tool will watch the changes in a .glicol file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the .glicol file
    #[arg(index=1)]
    file: String,

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
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut ringbuf_l = [0.0; RB_SIZE];
    let mut ringbuf_r = [0.0; RB_SIZE];
    let index = Arc::new(AtomicUsize::new(0));
    // let mut index_r = Arc::new(AtomicUsize::new(0));
    let index_clone = Arc::clone(&index);
    // let _index_r = Arc::clone(&index_r);
    let ptr_rb_left = Arc::new(AtomicPtr::<f32>::new( ringbuf_l.as_mut_ptr()));
    let ptr_rb_right = Arc::new(AtomicPtr::<f32>::new( ringbuf_r.as_mut_ptr()));
    let ptr_rb_left_clone = Arc::clone(&ptr_rb_left);
    let ptr_rb_right_clone = Arc::clone(&ptr_rb_right);

    let audio_thread = thread::spawn(move || {

        let args = Args::parse();
        let path = args.file;
        let device = args.device;
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

        match config.sample_format() {
            // ... other sample formats ...
            cpal::SampleFormat::F32 => run_audio::<f32>(&device, &config.into(), 
                ptr_rb_left_clone, ptr_rb_right_clone, index_clone, path),
            _ => unimplemented!(),
        }
    });

    let tick_rate = Duration::from_millis(10);
    let res = run_app(&mut terminal, tick_rate, ptr_rb_left, ptr_rb_right, index);

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

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    tick_rate: Duration, 
    left: Arc<AtomicPtr<f32>>, 
    right: Arc<AtomicPtr<f32>>,
    index: Arc<AtomicUsize>,
    // right: Arc<AtomicPtr<f32>>
) -> io::Result<()> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &left, &right, &index))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Esc = key.code {
                    return Ok(());
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // app.on_tick();
            last_tick = Instant::now();
        }
    }
}

// , play_audio: Arc<AtomicBool>
pub fn run_audio<T>(
    device: &cpal::Device, 
    config: &cpal::StreamConfig, 
    ringbuf_l: Arc<AtomicPtr<f32>>, 
    ringbuf_r: Arc<AtomicPtr<f32>>,
    index: Arc<AtomicUsize>,
    path: String
    // index_r: Arc<AtomicUsize>,
) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{

    // let path = "1.glicol".to_owned();
    let mut last_modified_time = metadata(&path)?.modified()?;

    let sr = config.sample_rate.0 as usize;

    let mut engine = Engine::<BLOCK_SIZE>::new();

    let mut code = String::new();
    let ptr = unsafe { code.as_bytes_mut().as_mut_ptr() };
    let code_ptr= Arc::new(AtomicPtr::<u8>::new(ptr));
    let code_len = Arc::new(AtomicUsize::new(code.len()));
    let has_update = Arc::new(AtomicBool::new(true));

    let _code_ptr = Arc::clone(&code_ptr);
    let _code_len = Arc::clone(&code_len);
    let _has_update = Arc::clone(&has_update);
    
    engine.set_sr(sr);

    let channels = 2 as usize; //config.channels as usize;

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
        
            if _has_update.load(Ordering::Acquire) {
                let ptr = _code_ptr.load(Ordering::Acquire);
                let len = _code_len.load(Ordering::Acquire);
                let encoded:&[u8] = unsafe { std::slice::from_raw_parts(ptr, len) };
                let code = std::str::from_utf8(encoded.clone()).unwrap().to_owned();
                engine.update_with_code(&code);
                _has_update.store(false, Ordering::Release);
            };

            let block_step = data.len() / channels;
            let blocks_needed = block_step / BLOCK_SIZE;
            let block_step = channels * BLOCK_SIZE;
            let mut rms: Vec<f32> = vec![0.0; channels];
            for current_block in 0..blocks_needed {
                let (block, _err_msg) = engine.next_block(vec![]);
                for i in 0..BLOCK_SIZE {
                    for chan in 0..channels {
                        rms[chan] += block[chan][i].powf(2.0);
                        let value: T = T::from_sample(block[chan][i]);
                        data[(i*channels+chan)+(current_block)*block_step] = value;
                    }
                }
            }
            rms = rms.into_iter().map(|x| (x / block_step as f32).sqrt() ).collect();
            // left rms[0] right rms[1]

            let ptr_l = ringbuf_l.load(Ordering::SeqCst);
            let ptr_r = ringbuf_r.load(Ordering::SeqCst);
            
            // let len = RB_SIZE;
            let idx = index.load(Ordering::SeqCst);
            unsafe {
                ptr_l.add(idx).write(rms[0]);
                ptr_r.add(idx).write(rms[1]);
            };
            index.store( (idx + 1) % RB_SIZE, Ordering::SeqCst); // from 0, 1, 2, RB_SIZE-1;
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
            code_ptr.store(unsafe {code.as_bytes_mut().as_mut_ptr() }, Ordering::SeqCst);
            code_len.store(code.len(), Ordering::SeqCst);
            has_update.store(true, Ordering::SeqCst);
        }
    }
}

fn ui<B: Backend>(
    f: &mut Frame<B>, 
    left: &Arc<AtomicPtr<f32>>, 
    right: &Arc<AtomicPtr<f32>>, 
    index: &Arc<AtomicUsize>
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(f.size());

    let mut data: [f32; RB_SIZE] = [0.0; RB_SIZE];
    let mut data2: [f32; RB_SIZE] = [0.0; RB_SIZE];
    let ptr = left.load(Ordering::SeqCst);
    let ptr2 = right.load(Ordering::SeqCst);
    let mut idx = index.load(Ordering::SeqCst); // let's say 20, while RB_size is 50
    for i in 0..RB_SIZE {
        let value = unsafe { ptr.add(idx).read() };
        data[RB_SIZE-1-i] = value;
        let value = unsafe { ptr2.add(idx).read() };
        data2[RB_SIZE-1-i] = value;
        if idx == 0 {
            idx = RB_SIZE - 1;// read from the tail
        } else {
            idx -= 1;
        }
    }

    // I keep this line; it's a bug; we need to convert the range
    // let leftvec = data.iter().map(|&x| x as u64).collect::<Vec<u64>>();

    let leftvec = data.iter().map(|&x| (x * 100.0) as u64).collect::<Vec<u64>>();
    let rightvec = data2.iter().map(|&x| (x * 100.0) as u64).collect::<Vec<u64>>();


    // let barchart = BarChart::default()
    // .block(Block::default().title("Data1").borders(Borders::ALL))
    // .data(&leftvec)
    // .bar_width(9)
    // .bar_style(Style::default().fg(Color::Yellow));
    // .value_style(Style::default().fg(Color::Black).bg(Color::Yellow));
    // f.render_widget(barchart, chunks[0]);


    let sparkline = Sparkline::default()
        .block(
            Block::default()
            .title("Left").borders(Borders::ALL)
        )
        .data(&leftvec)
        .style(Style::default().fg(Color::Blue));
    f.render_widget(sparkline, chunks[0]);

    let sparkline = Sparkline::default()
        .block(
            Block::default()
            .title("Right").borders(Borders::ALL)
        )
        .data(&rightvec)
        .style(Style::default().fg(Color::Red));
    f.render_widget(sparkline, chunks[1]);
}