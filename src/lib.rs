mod app;
mod thumbnailer;

use std::{ error::Error, num::NonZeroUsize, path::PathBuf, sync::mpsc, thread };

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
