mod app;
mod config;
mod mqtt;
mod secrets;

use app::Air1App;
use eframe::egui;
use tracing_subscriber::EnvFilter;

fn load_icon() -> egui::IconData {
    let image_data = include_bytes!("../Air1MQTT.png");
    let image = image::load_from_memory(image_data)
        .expect("Failed to load icon")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    
    egui::IconData {
        rgba,
        width,
        height,
    }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let version = env!("CARGO_PKG_VERSION");
    let git_count = env!("CARGO_PKG_VERSION_GIT");
    let full_version = format!("{}.r{}", version, git_count);
    let window_title = format!("Air 1 MQTT Monitor v{}", full_version);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 900.0])
            .with_resizable(true)
            .with_icon(load_icon()),
        vsync: true,
        multisampling: 0,
        ..Default::default()
    };

    eframe::run_native(
        &window_title,
        native_options,
        Box::new(|_cc| Ok(Box::new(Air1App::init()))),
    )
}
