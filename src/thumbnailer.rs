use std::{
    collections::VecDeque,
    ffi::OsStr,
    fs::{ self, DirEntry },
    path::Path,
    sync::mpsc::Sender,
    thread::JoinHandle,
    time::{ Duration, Instant },
};

use crate::*;

pub enum TimingData {
    SingleTime {
        name: String,
        duration: Duration,
    },
    TotalWithOfWhich {
        total_name: String,
        total_duration: Duration,
        ofwhich_name: String,
        ofwhich_duration: Duration,
    },
}

impl TimingData {
    pub fn single_time_from(name: &str, duration: Duration) -> Self {
        Self::SingleTime { name: String::from(name), duration }
    }

    pub fn total_of_which_from(
        total_name: &str,
        total_duration: Duration,
        ofwhich_name: &str,
        ofwhich_duration: Duration
    ) -> Self {
        Self::TotalWithOfWhich {
            total_name: String::from(total_name),
            total_duration,
            ofwhich_name: String::from(ofwhich_name),
            ofwhich_duration,
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            TimingData::SingleTime { name, duration } => { format!("{name} took {duration:?}") }
            TimingData::TotalWithOfWhich {
                total_name,
                total_duration,
                ofwhich_name,
                ofwhich_duration,
            } => {
                format!(
                    "\"{total_name}\" took {total_duration:?} (\"{ofwhich_name}\" took {ofwhich_duration:?} [{}%])",
                    (ofwhich_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0
                )
            }
        }
    }
}

pub trait DirEntryData {
    fn is_file(&self) -> bool;
    fn is_dir(&self) -> bool;
}

impl DirEntryData for DirEntry {
    fn is_file(&self) -> bool {
        self.metadata()
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    fn is_dir(&self) -> bool {
        self.metadata()
            .map(|m: fs::Metadata| m.is_dir())
            .unwrap_or(false)
    }
}

pub trait FileExtension {
    fn has_extension<S: AsRef<str>>(&self, extensions: &[S]) -> bool;
}

impl<P: AsRef<Path>> FileExtension for P {
    fn has_extension<S: AsRef<str>>(&self, extensions: &[S]) -> bool {
        if let Some(ref extension) = self.as_ref().extension().and_then(OsStr::to_str) {
            return extensions.iter().any(|x| x.as_ref().eq_ignore_ascii_case(extension));
        }

        false
    }
}

pub fn search_and_send<P>(path: P, sender: mpsc::Sender<PathBuf>) -> Result<(), Box<dyn Error>>
    where P: AsRef<Path>
{
    let mut dirs_to_scan: VecDeque<PathBuf> = fs
        ::read_dir(&path)?
        .into_iter()
        .filter_map(|val| {
            match val {
                Ok(entry) => Some(entry.path()),
                Err(_) => None,
            }
        })
        .collect();

    while let Some(dir) = dirs_to_scan.pop_front() {
        if let Ok(rd) = fs::read_dir(dir) {
            dirs_to_scan.append(
                &mut rd
                    .into_iter()
                    .filter_map(|val| {
                        match val {
                            Ok(entry) => {
                                if entry.is_dir() {
                                    return Some(entry.path());
                                }
                                if entry.is_file() {
                                    match sender.send(entry.path()) {
                                        Ok(_) => (),
                                        Err(err) =>
                                            log::debug!(
                                                "[searcher]: failed to send found entry on channel {err}"
                                            ),
                                    };
                                }
                                return None;
                            }
                            Err(_) => None,
                        }
                    })
                    .collect()
            );
        }
    }
    Ok(())
}

pub fn generate_thumbnail_from_image(
    path: PathBuf,
    max_x: u32,
    max_y: u32
) -> Result<image::DynamicImage, Box<dyn Error>> {
    let reader = image::io::Reader::open(&path)?;

    let dyn_image = reader.decode()?;
    let mut thumbnail = dyn_image.thumbnail(max_x, max_y);

    thumbnail = match thumbnail.color() {
        image::ColorType::L16 => image::DynamicImage::ImageLuma8(thumbnail.into_luma8()),
        image::ColorType::La16 => image::DynamicImage::ImageLumaA8(thumbnail.into_luma_alpha8()),
        image::ColorType::Rgb16 => image::DynamicImage::ImageRgb8(thumbnail.into_rgb8()),
        image::ColorType::Rgb32F => image::DynamicImage::ImageRgb8(thumbnail.into_rgb8()),
        image::ColorType::Rgba16 => image::DynamicImage::ImageRgba8(thumbnail.into_rgba8()),
        image::ColorType::Rgba32F => image::DynamicImage::ImageRgba8(thumbnail.into_rgba8()),
        _ => thumbnail,
    };

    Ok(thumbnail)
}

pub fn write_thumbnail(
    path: PathBuf,
    thumbs_dir: PathBuf,
    max_x: u32,
    max_y: u32
) -> Result<PathBuf, Box<dyn Error>> {
    let thumbnail = generate_thumbnail_from_image(path.clone(), max_x, max_y)?;
    let img_name: String = {
        if let Some(name) = path.file_name() {
            if let Some(str) = name.to_str() { String::from(str) } else { String::from("no_name") }
        } else {
            String::from("no_name")
        }
    };

    let thumb_path: PathBuf = match
        PathBuf::from(thumbs_dir.join(format!("{img_name}.png"))).exists()
    {
        false => { PathBuf::from(thumbs_dir.join(format!("{img_name}.png"))) }
        true => {
            let mut i: usize = 1;
            while PathBuf::from(thumbs_dir.join(format!("{img_name}_{i}.png"))).exists() {
                i += 1;
            }
            PathBuf::from(thumbs_dir.join(format!("{img_name}_{i}.png")))
        }
    };

    let format = image::ImageFormat::Png;
    thumbnail.save_with_format(&thumb_path, format)?;

    Ok(thumb_path)
}

pub struct SpawnedThumbnailer {
    pub handle: Option<thread::JoinHandle<()>>,
    pub sender: mpsc::Sender<AppToThumbnailer>,
    pub receiver: mpsc::Receiver<ThumbnailerToApp>,
}

impl SpawnedThumbnailer {
    pub fn new(
        handle: thread::JoinHandle<()>,
        sender: mpsc::Sender<AppToThumbnailer>,
        receiver: mpsc::Receiver<ThumbnailerToApp>
    ) -> Self {
        Self { handle: Some(handle), sender, receiver }
    }

    pub fn join(&mut self) -> Result<(), Box<dyn std::any::Any + Send + 'static>> {
        if let Some(handle) = self.handle.take() {
            return handle.join();
        }
        Ok(())
    }

    pub fn send(&self, msg: AppToThumbnailer) -> Result<(), mpsc::SendError<AppToThumbnailer>> {
        self.sender.send(msg)
    }
}

pub fn process_order(order: LoadData, thumb_data_tx: mpsc::Sender<ThumbnailerToApp>, order_id: usize) {
    let thread_name = String::from(thread::current().name().unwrap_or("thumbnailer-thread_{order_id}"));
    log::debug!("[{thread_name}]: spawned");

    let total_timer = Instant::now();

    // local constants
    let mut handles = Vec::<JoinHandle<()>>::new();

    match fs::remove_dir_all(&order.target_path) {
        Ok(_) => (),
        Err(err) => log::trace!("{err}"),
    }
    fs::create_dir_all(&order.target_path).unwrap();

    let (file_tx, file_rx) = mpsc::channel::<PathBuf>(); // searcher -> filter / distributor
    let mut file_senders = Vec::<mpsc::Sender<PathBuf>>::with_capacity(order.thread_count.get()); // filter / distributor -> processor_threads[]
    let (timing_tx, timing_rx) = mpsc::channel::<TimingData>(); // all threads -BENCHMARKS-> main thread

    // wlc message:
    log::debug!(
        "[{thread_name}]: generating thumbnails for all images in \"{}\" to \"{}\" with {} processing-threads...",
        order.path.display(),
        order.target_path.display(),
        order.thread_count.get()
    );

    // search thread
    {
        let name = format!("{order_id}-searcher");
        let builder = thread::Builder::new().name(name.clone());

        let timing_tx = timing_tx.clone();

        match
            builder.spawn(move || {
                let thread_name = String::from(thread::current().name().unwrap_or("unknown"));
                let timer_start = Instant::now();

                log::info!("[{thread_name}]: searching...");
                match search_and_send(&order.path, file_tx) {
                    Ok(_) => (),
                    Err(err) =>
                        log::error!("failed to read directory (path: \"{}\"; {err})", order.path.display()),
                }
                match
                    timing_tx.send(
                        TimingData::single_time_from(
                            "searching directories recursive",
                            timer_start.elapsed()
                        )
                    )
                {
                    Ok(_) => (),
                    Err(err) =>
                        log::warn!(
                            "[{thread_name}]: failed to send timings-data on channel ({err})"
                        ),
                }
                log::info!("[{thread_name}]: finished searching");
            })
        {
            Ok(handle) => {
                log::trace!("[{thread_name}]: spawned thread [{name}]");
                handles.push(handle);
            }
            Err(err) => log::error!("[{thread_name}]: failed to spawn thread [{name}] {err}"),
        }
    }

    // processing threads
    for i in 0..order.thread_count.get() {
        let name = String::from(format!("{order_id}-worker {i}"));
        let builder = thread::Builder::new().name(name.clone());

        let (tx, rx) = mpsc::channel::<PathBuf>();
        file_senders.push(tx);
        let timing_tx = timing_tx.clone();
        let thumb_data_tx = thumb_data_tx.clone();

        let target_path = order.target_path.clone();
        let (max_x, max_y) = (order.max_x, order.max_y);

        match
            builder.spawn(move || {
                let thread_name = String::from(thread::current().name().unwrap_or(&format!("{order_id}-unknown")));
                let timer_start = Instant::now();

                let mut work_dur: Duration = Duration::new(0, 0);
                let mut work_begin: Instant;

                'recv_loop: loop {
                    match rx.recv() {
                        Ok(path) => {
                            work_begin = Instant::now();

                            log::trace!("[{thread_name}]: rcvd {}", path.display());

                            match write_thumbnail(path.clone(), target_path.clone(), max_x, max_y) {
                                Ok(target_path) => {
                                    log::debug!(
                                        "[{thread_name}]: created thumbnail for {} at {}",
                                        path.display(),
                                        target_path.display()
                                    );

                                    match
                                        thumb_data_tx.send(
                                            ThumbnailerToApp::CreatedThumbnail(ThumbnailPaths {
                                                thumbnail: target_path.clone(),
                                                original: path.clone(),
                                            })
                                        )
                                    {
                                        Ok(_) => (),
                                        Err(err) => {
                                            log::warn!(
                                                "[{thread_name}]: failed to send ThumbnailPaths on channel ({err})"
                                            );
                                            break 'recv_loop; 
                                        }
                                            
                                    };
                                }
                                Err(err) => {
                                    log::trace!(
                                        "[{thread_name}]: failed to open file / decode image ({err})"
                                    );
                                }
                            }

                            work_dur += work_begin.elapsed();
                        }
                        Err(_) => {
                            break 'recv_loop;
                        }
                    }
                }

                match
                    timing_tx.send(
                        TimingData::total_of_which_from(
                            &format!("{thread_name} total"),
                            timer_start.elapsed(),
                            &format!("work"),
                            work_dur
                        )
                    )
                {
                    Ok(_) => (),
                    Err(err) =>
                        log::warn!("[{thread_name}]: failed to send path on channel ({err})"),
                }
                log::debug!("[{thread_name}]: finished processing");
            })
        {
            Ok(handle) => {
                log::trace!("[{thread_name}]: spawned thread [{name}]");
                handles.push(handle);
            }
            Err(err) => log::error!("[{thread_name}]: failed to spawn thread [{name}] {err}"),
        }
    }

    // receiving + filtering + distributing thread
    {
        let name = format!("{order_id}-filter/distributor");
        let builder = thread::Builder::new().name(name.clone());

        let target_path = order.target_path.clone();
        let timing_tx = timing_tx.clone();

        match
            builder.spawn(move || {
                let thread_name = String::from(thread::current().name().unwrap_or("unknown"));
                let timer_start = Instant::now();

                let mut iter_count = 0;

                log::trace!("[{thread_name}]: rcving, filtering and distributing...");
                'recv_loop: loop {
                    match file_rx.recv() {
                        Ok(path) => {
                            if path == target_path {
                                continue 'recv_loop;
                            }

                            if let Some(path_str) = path.to_str() {
                                if path_str.contains("$RECYCLE.BIN") {
                                    continue 'recv_loop;
                                }
                            }

                            if
                                let Some(tx) = file_senders.get(
                                    iter_count % order.thread_count.get()
                                )
                            {
                                match tx.send(path) {
                                    Ok(_) => (),
                                    Err(err) =>
                                        log::warn!(
                                            "[{thread_name}]: failed to send path on channel ({err})"
                                        ),
                                };
                            } else {
                                log::warn!(
                                    "No sender in \"file_senders\" at {} (len: {})",
                                    iter_count % order.thread_count.get(),
                                    file_senders.len()
                                );
                            }
                            iter_count += 1;
                        }
                        Err(_) => {
                            break 'recv_loop;
                        }
                    }
                }
                drop(file_senders);
                match
                    timing_tx.send(
                        TimingData::single_time_from(
                            "receiving and distributing files",
                            timer_start.elapsed()
                        )
                    )
                {
                    Ok(_) => (),
                    Err(err) =>
                        log::warn!(
                            "[{thread_name}]: failed to send timings-data on channel ({err})"
                        ),
                }

                log::trace!("[{thread_name}]: finished");
            })
        {
            Ok(handle) => {
                log::trace!("[{thread_name}]: spawned thread [{name}]");
                handles.push(handle);
            }
            Err(err) => log::error!("[{thread_name}]: failed to spawn thread [{name}] {err}"),
        }
    }

    for handle in &handles {
        handle.thread().unpark();
    }

    // threads are now doing their work

    for handle in handles {
        let joined_thread_name: String = String::from(handle.thread().name().unwrap_or("unknown"));
        log::trace!("[{thread_name}]: joining thread [{joined_thread_name}]");
        match handle.join() {
            Err(err) =>
                log::error!(
                    "[{thread_name}]: failed to join thread [{joined_thread_name}] {err:?}"
                ),
            _ => (),
        };
    }

    // every thread has finished now

    match timing_tx.send(TimingData::single_time_from("total time", total_timer.elapsed())) {
        Ok(_) => (),
        Err(err) => log::warn!("[{thread_name}]: failed to send timings-data on channel ({err})"),
    }

    drop(timing_tx);
    while let Ok(timing_data) = timing_rx.recv() {
        log::debug!("[{thread_name}]: {}", timing_data.to_string());
    }
}

pub fn spawn_thumbnailer_thread() -> Result<SpawnedThumbnailer, Box<dyn Error>> {
    let (client_tx, thumbnailer_rx) = mpsc::channel::<AppToThumbnailer>();
    let (thumbnailer_tx, client_rx) = mpsc::channel::<ThumbnailerToApp>();

    let builder = thread::Builder::new().name(String::from("thumbnailer-thread"));

    let handle = builder.spawn(move || {
        let thread_name = String::from(thread::current().name().unwrap_or("thumbnailer-thread"));
        log::debug!("[{thread_name}]: spawned");

        let mut handles: Vec<JoinHandle<()>> = vec![];

        // orp = order-processor
        let mut orp_counter: usize = 0;

        'thread_loop: loop {
            let rcvd = match thumbnailer_rx.recv() {
                Ok(msg) => msg,
                Err(_) => {
                    break 'thread_loop;
                }
            };

            log::debug!("[{thread_name}]: received {rcvd:?}");

            match rcvd {
                AppToThumbnailer::ThumbnailOrder(order) => {
                    let sender = Sender::clone(&thumbnailer_tx);

                    let order_id = orp_counter;

                    handles.push(
                        thread::Builder
                            ::new()
                            .name(format!("order-processor-{order_id}"))
                            .spawn(move || {
                                process_order(order, sender, order_id);
                            })
                            .unwrap()
                    );

                    orp_counter += 1;
                }
                AppToThumbnailer::KillCmd => {
                    log::debug!("[{thread_name}]: killing thread...");
                    break 'thread_loop;
                }
            }
        }

        // for handle in handles {
        //     match handle.join() {
        //         Ok(_) => (),
        //         Err(_) => log::debug!("[{thread_name}]: failed to join thread"),
        //     };
        // }
    })?;

    return Ok(SpawnedThumbnailer::new(handle, client_tx, client_rx));
}
