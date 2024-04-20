use crate::BLOCK_SIZE;
use anyhow::Context;
use dirs;
use glicol::Engine;
use rayon::prelude::*;
use std::{fs::File, path::Path};
use symphonia::core::{
    audio::Signal, codecs::DecoderOptions, formats::FormatReader, io::MediaSourceStream,
    probe::Hint,
};
use tracing::error;
use walkdir::WalkDir;

pub fn load_samples_from_env(engine: &mut Engine<BLOCK_SIZE>) {
    let key = "GLICOL_CLI_SAMPLES_PATH";

    if let Some(paths) = std::env::var_os(key) {
        for path in std::env::split_paths(&paths) {
            if let Err(error) = load_samples_from_dir(engine, &path) {
                error!(?path, "failed to load samples: {error:#}");
            }
        }
    }
}

fn expand_home_dir(path: &str) -> std::path::PathBuf {
    let path_buf = if path.starts_with("~") {
        let without_tilde = &path[1..];
        if let Some(home_dir) = dirs::home_dir() {
            home_dir.join(without_tilde.trim_start_matches('/'))
        } else {
            std::path::PathBuf::from(path)
        }
    } else {
        std::path::PathBuf::from(path)
    };
    if path_buf.is_relative() {
        std::env::current_dir().unwrap().join(path_buf)
    } else {
        path_buf
    }
}

fn load_samples_from_dir(
    engine: &mut Engine<BLOCK_SIZE>,
    dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let dir = expand_home_dir(dir.as_ref().to_str().unwrap());
    let walk_dir = WalkDir::new(&dir)
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|s| !s.starts_with('.'))
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            entry.ok().filter(|entry| {
                AsRef::<std::path::Path>::as_ref(&entry.file_name())
                    .extension()
                    .map(|ext| ext == "wav" || ext == "mp3" || ext == "ogg")
                    .unwrap_or(false)
            })
        })
        .map(|entry| (entry.path().to_path_buf(), entry.depth()))
        .collect::<Vec<_>>();
    // TODO: show available samples
    // println!("Found {} samples from {:?}", walk_dir.len(), &dir);
    for (sample, name) in walk_dir
        .par_iter()
        .filter_map(|(path, depth)| {
            let prefix = if *depth == 2 {
                path.parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            } else {
                ""
            };

            let name = format!(
                "\\{}{}",
                prefix,
                path.file_stem().unwrap().to_str().unwrap()
            );

            load_sample(path).ok().zip(Some(name))
        })
        .collect::<Vec<_>>()
    {
        let sample_buffer = Box::leak(sample.buffer.into_boxed_slice());
        println!("Adding sample: {}", name);
        engine.add_sample(&name, sample_buffer, sample.channels, sample.sr);
    }

    Ok(())
}

struct Sample {
    buffer: Vec<f32>,
    sr: usize,
    channels: usize,
}

fn load_sample(path: impl AsRef<Path>) -> anyhow::Result<Sample> {
    let mut hint = Hint::new();

    if let Some(extension) = path.as_ref().extension() {
        if let Some(extension_str) = extension.to_str() {
            hint.with_extension(extension_str);
        }
    }

    let source = Box::new(File::open(path)?);

    let mss = MediaSourceStream::new(source, Default::default());

    match symphonia::default::get_probe().format(
        &hint,
        mss,
        &Default::default(),
        &Default::default(),
    ) {
        Ok(probed) => decode(probed.format, &DecoderOptions { verify: false })
            .context("Couldn't decode sample"),
        Err(err) => {
            anyhow::bail!("input format not supported: {err}");
        }
    }
}

fn decode(
    mut reader: Box<dyn FormatReader>,
    decode_opts: &DecoderOptions,
) -> anyhow::Result<Sample> {
    let track = reader.default_track().unwrap();
    let track_id = track.id;

    let sr = track
        .codec_params
        .sample_rate
        .ok_or(anyhow::anyhow!("Couldn't get sample rate"))? as usize;

    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, decode_opts)?;

    let channels = track
        .codec_params
        .channels
        .ok_or(anyhow::anyhow!("Couldn't get channel info"))?
        .count();

    if channels > 2 {
        anyhow::bail!("unsupported channel number");
    }

    let mut channel_0: Vec<f32> = vec![];
    let mut channel_1: Vec<f32> = vec![];

    let result = loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(err) => break Err(err),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(err) => break Err(err),
        };

        let mut buffer = decoded.make_equivalent::<f32>();

        decoded.convert(&mut buffer);
        channel_0.extend(buffer.chan(0));

        if channels == 2 {
            channel_1.extend(buffer.chan(1));
        }
    };

    match result {
        Err(symphonia::core::errors::Error::IoError(err))
            if err.kind() == std::io::ErrorKind::UnexpectedEof
                && err.to_string() == "end of stream" =>
        {
            Ok(())
        }
        _ => result,
    }?;

    // if mono then channel_1 would be empty, so no change here
    channel_0.extend(channel_1);

    Ok(Sample {
        buffer: channel_0,
        sr,
        channels,
    })
}
