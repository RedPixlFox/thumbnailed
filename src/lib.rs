mod app;
mod thumbnailer;

use std::{
    error::Error,
    fs,
    num::NonZeroUsize,
    os::windows::fs::MetadataExt,
    path::{ Path, PathBuf },
    sync::mpsc,
    thread,
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
