use thumbnailed::ThumbnailedApp;

const LOG_LEVEL: &str = "debug";

fn init_logger() {
    let env = env_logger::Env
        ::default()
        .filter_or("MY_LOG_LEVEL", LOG_LEVEL)
        .write_style_or("MY_LOG_STYLE", "always");

    env_logger::init_from_env(env);
}

fn main() -> Result<(), eframe::Error> {
    init_logger();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([600.0, 800.0]),
        persist_window: true,
        
        ..Default::default()
    };
    eframe::run_native(
        "Thumbnailed",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Box::<ThumbnailedApp>::default()
        })
    )
}
