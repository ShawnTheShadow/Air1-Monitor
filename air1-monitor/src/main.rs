mod app;
mod config;
mod mqtt;
mod secrets;

use app::Air1App;
use eframe::egui;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Air 1 MQTT Monitor",
        native_options,
        Box::new(|_cc| Box::new(Air1App::init())),
    )
}
