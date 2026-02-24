mod app;
mod config;
mod mqtt;
mod secrets;
mod ui;

use gtk4::prelude::*;
use tracing_subscriber::EnvFilter;

fn main() -> gtk4::glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let version = env!("CARGO_PKG_VERSION");
    let git_count = env!("CARGO_PKG_VERSION_GIT");
    let full_version = format!("{}.r{}", version, git_count);
    let window_title = format!("Air 1 MQTT Monitor v{full_version}");

    let gtk_app = gtk4::Application::builder()
        .application_id("com.air1.monitor")
        .build();

    gtk_app.connect_activate(move |gtk_app| {
        let state = std::rc::Rc::new(std::cell::RefCell::new(app::Air1App::init()));
        ui::build_ui(gtk_app, state, &window_title);
    });

    gtk_app.run()
}
