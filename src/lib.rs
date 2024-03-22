mod app;
mod thumbnailer;

use std::{
    collections::VecDeque,
    error::Error,
    fs,
    num::NonZeroUsize,
    os::windows::fs::MetadataExt,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{ Duration, Instant },
};

pub use app::ThumbnailedApp;

#[derive(Debug)]
pub enum ThumbnailerToApp {
    CreatedThumbnail(ThumbnailPaths),
    Status(ThumbnailerStatus),
}

unsafe impl Send for ThumbnailerToApp {}

#[derive(Debug)]
pub enum AppToThumbnailer {
    ThumbnailOrder(LoadData),
    KillCmd,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ThumbnailPaths {
    pub thumbnail: PathBuf,
    pub original: PathBuf,
}

#[derive(Debug)]
pub enum ThumbnailerStatus {
    Finished,
    Failed(Option<Box<dyn Error>>),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct LoadData {
    path: PathBuf,
    target_path: PathBuf,
    thread_count: NonZeroUsize,
    max_x: u32,
    max_y: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LoadDialougeData {
    path: String,
    thread_count: usize,
    max_x: u32,
    max_y: u32,
}

impl Default for LoadDialougeData {
    fn default() -> Self {
        Self { path: String::from(r"C:\"), thread_count: 8, max_x: 128, max_y: 128 }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct StorageSize {
    bytes: u64,
}

impl StorageSize {
    pub fn new(bytes: u64) -> Self {
        Self { bytes }
    }

    pub fn get_bytes_mut(&mut self) -> &mut u64 {
        &mut self.bytes
    }

    pub fn get_bytes(&self) -> u64 {
        self.bytes
    }

    pub fn in_kilobytes(&self) -> f64 {
        (self.bytes as f64) * 0.001
    }

    pub fn in_megabytes(&self) -> f64 {
        (self.bytes as f64) * 0.000001
    }

    pub fn in_gigabytes(&self) -> f64 {
        (self.bytes as f64) * 0.000000001
    }

    pub fn in_terabytes(&self) -> f64 {
        (self.bytes as f64) * 0.000000000001
    }

    /// will return None, if directory doesn't exist
    pub fn from_dir(path: PathBuf) -> Option<Self> {
        if path.exists() {
            let mut dirs_to_scan = vec![path];
            let mut bytes: u64 = 0;
            loop {
                match dirs_to_scan.pop() {
                    Some(path) => {
                        if let Ok(rd) = fs::read_dir(&path) {
                            for entry in rd {
                                if let Ok(entry) = &entry {
                                    if let Ok(metadata) = fs::metadata(&entry.path()) {
                                        if metadata.is_file() {
                                            bytes += metadata.file_size();
                                            continue;
                                        }
                                        if metadata.is_dir() {
                                            dirs_to_scan.push(entry.path());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        return Some(Self { bytes });
                    }
                }
            }
        } else {
            None
        }
    }

    pub fn from_file(path: PathBuf) -> Option<Self> {
        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.is_file() {
                return Some(Self { bytes: metadata.file_size() });
            }
        }
        None
    }
}

impl Default for StorageSize {
    fn default() -> Self {
        Self { bytes: 0 }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timings {
    pub avg_delta: Duration,
    pub last_delta: Duration,
    last_durs: VecDeque<(Instant, Duration)>,
    averaging_dur: Duration,
}

impl Timings {
    pub fn new(averaging_duration: Duration) -> Self {
        Self {
            last_durs: VecDeque::new(),
            averaging_dur: averaging_duration,
            avg_delta: Duration::ZERO,
            last_delta: Duration::ZERO,
        }
    }

    pub fn frame_begin(&mut self) {
        while
            self.last_durs
                .back()
                .is_some_and(|frame_stats| { frame_stats.0.elapsed() > self.averaging_dur }) &&
            self.last_durs.len() > 1
        {
            self.last_durs.pop_back();
        }

        // all valid values are here :)

        self.last_delta = match self.last_durs.front() {
            Some(last_data) => last_data.0.elapsed(),
            None => Duration::ZERO,
        };

        self.last_durs.push_front((Instant::now(), self.last_delta));

        self.avg_delta = {
            self.last_durs
                .iter()
                .map(|elem| { elem.1 })
                .sum::<Duration>()
                .div_f64(self.last_durs.len() as f64)
        };
    }
}
