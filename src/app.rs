use std::{
    fs,
    num::NonZeroUsize,
    os::windows::fs::MetadataExt,
    path::PathBuf,
    process::Command,
    thread,
    time::{ Duration, Instant },
};

use eframe::egui as egui;

use crate::*;

pub struct ThumbnailedApp {
    pub thumbnails: Vec<ThumbnailPaths>,
    pub load_data: Option<LoadData>,

    pub thumbnail_path: PathBuf,

    pub load_dialouge_data: LoadDialougeData,
    pub show_load_dialouge: bool,

    pub allowed_to_close: bool,
    pub show_close_dialouge: bool,

    pub update_gallery: bool,
    pub gallery_cache_size: StorageSize,
    pub total_cache_size: StorageSize,
    pub last_cache_size_update: Instant,

    pub show_path_on_hover: bool,

    pub thumbnailer: Option<thumbnailer::SpawnedThumbnailer>,
}

impl ThumbnailedApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.

        let mut app: Self = Default::default();

        // spawn
        match thumbnailer::spawn_thumbnailer_thread() {
            Ok(spwnd_thumbnailer) => {
                app.thumbnailer = Some(spwnd_thumbnailer);
            }
            Err(err) => panic!("failed to spawn [thumbnailer]-thread ({err})"),
        }

        return app;
    }

    pub fn update_gallery_cache_size(&mut self) {
        let mut size = 0;

        for thumbnail_data in &self.thumbnails {
            match fs::metadata(&thumbnail_data.thumbnail) {
                Ok(metadata) => {
                    size += metadata.file_size();
                }
                Err(_) => (),
            }
        }

        self.gallery_cache_size = StorageSize::new(size);
    }

    pub fn update_total_cache_size(&mut self) {
        self.total_cache_size = StorageSize::from_dir(
            self.thumbnail_path.clone()
        ).unwrap_or_default();
    }

    const CACHE_SIZE_UPDATE_INTERVAL: Duration = Duration::from_millis(200);
}

impl Default for ThumbnailedApp {
    fn default() -> Self {
        Self {
            thumbnails: Vec::new(),
            load_data: None,
            thumbnail_path: PathBuf::from("tmp/thumbs-cache"),
            load_dialouge_data: LoadDialougeData {
                path: String::new(),
                thread_count: 4,
                max_x: 128,
                max_y: 128,
            },
            show_load_dialouge: false,
            thumbnailer: None,
            allowed_to_close: false,
            show_close_dialouge: false,
            update_gallery: true,
            gallery_cache_size: StorageSize::new(0),
            total_cache_size: StorageSize::new(0),
            last_cache_size_update: Instant::now(),
            show_path_on_hover: true,
        }
    }
}

impl eframe::App for ThumbnailedApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if Instant::now() - self.last_cache_size_update > Self::CACHE_SIZE_UPDATE_INTERVAL {
            self.update_total_cache_size();
            self.update_gallery_cache_size();
            self.last_cache_size_update = Instant::now();
        }

        if self.thumbnailer.is_none() {
            match thumbnailer::spawn_thumbnailer_thread() {
                Ok(spwnd_thumbnailer) => {
                    self.thumbnailer = Some(spwnd_thumbnailer);
                }
                Err(err) => panic!("failed to spawn [thumbnailer]-thread ({err})"),
            }
        }

        if self.update_gallery {
            if let Some(thumbnailer) = &self.thumbnailer {
                if let Ok(msg) = thumbnailer.receiver.try_recv() {
                    match msg {
                        ThumbnailerToApp::CreatedThumbnail(data) => {
                            self.thumbnails.push(data);
                        }
                        ThumbnailerToApp::Status(status) => {
                            log::debug!("received status update from thumbnailer: {status:?}");
                            match status {
                                ThumbnailerStatus::Finished =>
                                    log::debug!("thumbnailer has finished creating thumbnails"),
                                ThumbnailerStatus::Failed(err) => {
                                    match err {
                                        Some(err) =>
                                            log::error!("thumbnailer returned an error ({err})"),
                                        None =>
                                            log::error!("thumbnailer returned an unknown error"),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.thumbnails.sort_by(|a, b| { a.original.cmp(&b.original) });

            self.update_gallery_cache_size();
        }

        // top panel
        egui::TopBottomPanel::new(egui::panel::TopBottomSide::Top, "top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::menu::menu_button(ui, "Options", |ui| {
                    ui.checkbox(&mut self.show_path_on_hover, format!("show path on hover"));
                    ui.checkbox(&mut self.update_gallery, format!("update gallery"));

                    if ui.button("clear cache").clicked() {
                        match fs::remove_dir_all(&self.thumbnail_path) {
                            Ok(_) => log::debug!("cleared cache"),
                            Err(err) => log::debug!("failed to clear cache ({err})"),
                        }

                        match fs::create_dir_all(&self.thumbnail_path) {
                            Ok(_) => log::debug!("created cache-directory successfully"),
                            Err(_) => log::debug!("failed to create cache-directory"),
                        }

                        self.thumbnails.clear();

                        ui.close_menu();
                    }

                    if ui.button("terminate tasks").clicked() {
                        if let Some(thumbnailer) = &mut self.thumbnailer {
                            let _ = thumbnailer.send(AppToThumbnailer::KillCmd);
                            let _ = thumbnailer.join();
                        }
                        match thumbnailer::spawn_thumbnailer_thread() {
                            Ok(spwnd_thumbnailer) => {
                                self.thumbnailer = Some(spwnd_thumbnailer);
                            }
                            Err(err) => panic!("failed to spawn [thumbnailer]-thread ({err})"),
                        }

                        ui.close_menu();
                    }

                    ui.separator();

                    egui::widgets::global_dark_light_mode_buttons(ui);

                    if ui.button("exit").clicked() {
                        ui.close_menu();
                        self.allowed_to_close = true;
                        self.show_close_dialouge = false;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // menu logic

                    if ctx.input(|i| i.viewport().close_requested()) {
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if ui.button("load").clicked() {
                        // TODO: add logic and "load dialog"

                        self.show_load_dialouge = true;
                    }
                })
            })
        });

        // TODO: create bottom panel
        egui::TopBottomPanel
            ::new(egui::panel::TopBottomSide::Bottom, "bottom panel")
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.label(format!("{} items", self.thumbnails.len()));

                    // ui.add_space(4.0);
                    ui.separator();
                    // ui.add_space(4.0);

                    ui.label(format!("cache: [{:.2} MB]", self.total_cache_size.in_megabytes()));
                    // ui.add(egui::ProgressBar::new(0.45).desired_height(12.0))
                })
            });

        // image view
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for thumbnail_paths in self.thumbnails.iter() {
                        if let Some(thumb_path_str) = thumbnail_paths.thumbnail.to_str() {
                            let (max_x, max_y) = match &self.load_data {
                                Some(load_data) => (load_data.max_x, load_data.max_y),
                                None => (128, 128),
                            };

                            if
                                ui
                                    .add_sized(
                                        [max_x as f32, max_y as f32],
                                        egui::Image
                                            ::new(
                                                egui::ImageSource::Uri(
                                                    std::borrow::Cow::Borrowed(
                                                        &format!("file://{thumb_path_str}")
                                                    )
                                                )
                                            )
                                            .sense(egui::Sense::click())
                                            .max_size(egui::Vec2 {
                                                x: max_x as f32,
                                                y: max_y as f32,
                                            })
                                    )
                                    .on_hover_text_at_pointer(
                                        format!("{}", thumbnail_paths.original.display())
                                    )
                                    .clicked()
                            {
                                if let Some(orig_path_str) = thumbnail_paths.original.to_str() {
                                    #[cfg(target_os = "windows")]
                                    {
                                        Command::new("explorer")
                                            .arg("/select,")
                                            .arg(orig_path_str)
                                            .spawn()
                                            .unwrap();
                                    }
                                }
                            }
                        }
                    }
                });
            });
        });

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.allowed_to_close {
                // do nothing - we will close
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);

                self.show_close_dialouge = true;
            }
        }

        if self.show_load_dialouge {
            egui::Window
                ::new("LOAD")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let slider_max = match thread::available_parallelism() {
                            Ok(max) => max.get(),
                            Err(_) => 24,
                        };

                        ui.add(
                            egui::Slider
                                ::new(&mut self.load_dialouge_data.thread_count, 1..=slider_max)
                                .text("threads")
                        )
                    });

                    ui.vertical(|ui| {
                        ui.label("path to root directory:");
                        ui.text_edit_singleline(&mut self.load_dialouge_data.path);
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.show_load_dialouge = false;
                        }

                        if ui.button("Load").clicked() {
                            let thread_count = match
                                NonZeroUsize::new(self.load_dialouge_data.thread_count)
                            {
                                Some(some) => some,
                                None => NonZeroUsize::MIN,
                            };

                            if PathBuf::from(self.load_dialouge_data.path.clone()).exists() {
                                self.load_data = Some(LoadData {
                                    path: PathBuf::from(self.load_dialouge_data.path.clone()),
                                    target_path: self.thumbnail_path.clone(),
                                    thread_count,
                                    max_x: self.load_dialouge_data.max_x,
                                    max_y: self.load_dialouge_data.max_y,
                                });

                                if let Some(spawned_thumbnailer) = &self.thumbnailer {
                                    match
                                        spawned_thumbnailer.send(
                                            AppToThumbnailer::ThumbnailOrder(LoadData {
                                                path: PathBuf::from(
                                                    self.load_dialouge_data.path.clone()
                                                ),
                                                target_path: self.thumbnail_path.clone(),
                                                thread_count,
                                                max_x: self.load_dialouge_data.max_x,
                                                max_y: self.load_dialouge_data.max_y,
                                            })
                                        )
                                    {
                                        Ok(_) => log::debug!("sent thumbnail order to thumbnailer"),
                                        Err(err) =>
                                            log::error!(
                                                "failed to send thumbnail order on channel ({err})"
                                            ),
                                    };
                                } else {
                                    log::error!("no thumbnailer found");
                                }

                                self.show_load_dialouge = false;
                            } else {
                                self.load_dialouge_data.path = String::from("path doesnt exist");
                            }
                        }
                    });
                });
        }

        if self.show_close_dialouge {
            egui::Window
                ::new("EXIT?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("No").clicked() {
                            self.show_close_dialouge = false;
                            self.allowed_to_close = false;
                        }

                        if ui.button("Yes").clicked() {
                            self.show_close_dialouge = false;
                            self.allowed_to_close = true;
                            if let Some(thumbnailer) = &mut self.thumbnailer {
                                thumbnailer.send(AppToThumbnailer::KillCmd).unwrap();
                                thumbnailer.join().unwrap();
                            }
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        }

        ctx.request_repaint();
    }
}
