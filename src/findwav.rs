use std::path::{Path, PathBuf};
use walkdir::{WalkDir, DirEntry};
use hound::{WavReader, SampleFormat};

use hashbrown::HashMap;
use std::fs::{File, metadata};
use std::io::{BufRead, BufReader};

pub fn find_wav_files(path: &str) //, mut sample_dict: HashMap<String, (&'static [f32], usize, usize)>)
 -> HashMap<String, (Vec<f32>, usize, usize)> {
    let walker = WalkDir::new(path).into_iter();
    // let mut wav_files = HashMap::new();

    let mut sample_dict = HashMap::new();

    for entry in walker.filter_entry(|e| e.file_type().is_file() || e.file_type().is_dir()) {
        let entry = entry.unwrap();
        if is_wav(&entry) {
            // wav_files.push(entry.path().to_path_buf());
            let file = File::open(entry.path().to_path_buf()).unwrap();
            let buf_reader = BufReader::new(file);
            let mut reader = WavReader::new(buf_reader).unwrap();
    
            let p = std::path::Path::new(&path);
            let name;
            
            if let (Some(file_stem), Some(extension)) = (p.file_stem(), p.extension()) {
                if extension == "wav" {
                    name = file_stem.to_str().map(|s| s.to_string());
                }
            };
    
            match reader.spec().sample_format {
                SampleFormat::Int => {
                    match reader.spec().bits_per_sample {
                        16 => {
                            let mut v = vec![];
                            for sample in reader.samples::<i16>() {
                                v.push(sample.unwrap() as f32 / (2_i16.pow(15) as f32))
                            }
                            sample_dict.insert(name.unwrap(), (&v, v.len(), reader.spec().channels as usize));
                        },
                        24 => {
                            let mut v = vec![];
                            for sample in reader.samples::<i16>() {
                                v.push(sample.unwrap() as f32 / 8_388_607.0)
                            }
                            sample_dict.insert(name.unwrap(), (&v, v.len(), reader.spec().channels as usize));
                        },
                        _ => {
                            
                            let mut v = vec![];
                            for sample in reader.samples::<i16>() {
                                v.push(sample.unwrap() as f32 / (2_i32.pow(31) as f32))
                            }
                            sample_dict.insert(name.unwrap(), (&v, v.len(), reader.spec().channels as usize));
                        }
                    }
                },
                SampleFormat::Float => {}
            }
        }
    }
    sample_dict
}

fn is_wav(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.ends_with(".wav"))
        .unwrap_or(false)
}