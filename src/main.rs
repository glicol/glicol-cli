use anyhow::Result;
use clap::Parser;

use std::fs::{File, metadata};
use std::io::{BufRead, BufReader};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SizedSample,
};

use glicol::Engine;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, AtomicPtr, Ordering};

use chrono::{DateTime, Utc};

/// millisecond duration to watch the changes
// #[arg(short, long)]
// dur: u64,

/// Glicol cli tool. This tool will watch the changes in a .glicol file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to the .glicol file
    #[arg(short, long, index=1)]
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

const BLOCK_SIZE: usize = 128;

fn main() -> Result<()> {
    let args = Args::parse();

    let path = args.file;
    // let path = args.file;
    // let dur = args.dur;
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
    println!("Output device: {}", device.name()?);

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);

    match config.sample_format() {
        cpal::SampleFormat::I8 => run::<i8>(&device, &config.into(), path),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into(), path),
        // cpal::SampleFormat::I24 => run::<I24>(&device, &config.into()),
        cpal::SampleFormat::I32 => run::<i32>(&device, &config.into(), path),
        // cpal::SampleFormat::I48 => run::<I48>(&device, &config.into()),
        cpal::SampleFormat::I64 => run::<i64>(&device, &config.into(), path),
        cpal::SampleFormat::U8 => run::<u8>(&device, &config.into(), path),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into(), path),
        // cpal::SampleFormat::U24 => run::<U24>(&device, &config.into()),
        cpal::SampleFormat::U32 => run::<u32>(&device, &config.into(), path),
        // cpal::SampleFormat::U48 => run::<U48>(&device, &config.into()),
        cpal::SampleFormat::U64 => run::<u64>(&device, &config.into(), path),
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into(), path),
        cpal::SampleFormat::F64 => run::<f64>(&device, &config.into(), path),
        sample_format => panic!("Unsupported sample format '{sample_format}'"),
    }

}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig, path: String) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32>,
{

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

            let blocks_needed = data.len() / 2 / BLOCK_SIZE;
            let block_step = channels * BLOCK_SIZE;
            for current_block in 0..blocks_needed {
                let (block, _err_msg) = engine.next_block(vec![]);
                for i in 0..BLOCK_SIZE {
                    for chan in 0..channels {
                        let value: T = T::from_sample(block[chan][i]);
                        data[(i*channels+chan)+(current_block)*block_step] = value;
                    }
                }
            }
        },
        err_fn,
        None,
    )?;
    stream.play()?;

    loop {
        std::thread::sleep(Duration::from_millis(100));
        let modified_time = metadata(&path)?.modified()?;
        if modified_time != last_modified_time {
            last_modified_time = modified_time;
            let file = File::open(&path)?;
            let reader = BufReader::new(file);
            code = "".to_owned();
            for line in reader.lines() {
                code.push_str(&line?);
                code.push_str("\n");
            }
            let current_time = SystemTime::now();
            let unix_time = current_time.duration_since(UNIX_EPOCH)
                .expect("Time went backwards");
            let system_time = UNIX_EPOCH + unix_time;
            let datetime = DateTime::<Utc>::from(system_time);
            println!("```");
            println!("\n// utc time: {} \n", datetime.format("%Y-%m-%d %H:%M:%S").to_string());
            println!("{}", code);
            println!("```");
            code_ptr.store(unsafe {code.as_bytes_mut().as_mut_ptr() }, Ordering::SeqCst);
            code_len.store(code.len(), Ordering::SeqCst);
            has_update.store(true, Ordering::SeqCst);
        }
    }
}