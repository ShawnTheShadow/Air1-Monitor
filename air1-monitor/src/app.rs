use eframe::{App, egui};
use std::{sync::mpsc, thread::JoinHandle, time::Instant};
use tracing::warn;

use crate::{config, mqtt, secrets};

enum TestResult {
    Ok,
    Err(String),
}

#[derive(Default, Clone, Debug)]
struct Metrics {
    pm1: Option<f64>,
    pm25: Option<f64>,
    pm10: Option<f64>,
    tvoc: Option<f64>,
    co2: Option<f64>,
    temp: Option<f64>,
    humidity: Option<f64>,
    battery: Option<f64>,
    last_topic: Option<String>,
    last_update: Option<Instant>,
}

pub enum MqttEvent {
    Connected,
    Disconnected(String),
    Metric {
        topic: String,
        value: f64,
        kind: String,
    },
    Status(String),
}

pub struct Air1App {
    pub cfg_paths: config::ConfigPaths,
    pub cfg: config::AppConfig,
    pub password: Option<String>,
    pub status: String,
    pub last_save: Option<Instant>,
    pub keyring_unavailable: bool,
    pub testing: bool,
    test_rx: mpsc::Receiver<TestResult>,
    test_tx: mpsc::Sender<TestResult>,
    mqtt_rx: mpsc::Receiver<MqttEvent>,
    mqtt_tx: mpsc::Sender<MqttEvent>,
    metrics: Metrics,
    connected: bool,
    mqtt_handle: Option<JoinHandle<()>>,
    mqtt_stop: Option<mpsc::Sender<()>>,
}

impl Default for Air1App {
    fn default() -> Self {
        let cfg_paths = config::ConfigPaths::default();
        let cfg = config::AppConfig::default();
        let (test_tx, test_rx) = mpsc::channel();
        let (mqtt_tx, mqtt_rx) = mpsc::channel();
        Self {
            cfg_paths,
            cfg,
            password: None,
            status: String::new(),
            last_save: None,
            keyring_unavailable: false,
            testing: false,
            test_rx,
            test_tx,
            mqtt_rx,
            mqtt_tx,
            metrics: Metrics::default(),
            connected: false,
            mqtt_handle: None,
            mqtt_stop: None,
        }
    }
}

impl Air1App {
    pub fn init() -> Self {
        let cfg_paths = match config::ConfigPaths::new() {
            Ok(paths) => paths,
            Err(err) => {
                warn!("config path error: {err:?}");
                let mut fallback = Self::default();
                fallback.status = format!("Config path error: {err:#}");
                fallback.keyring_unavailable = true;
                return fallback;
            }
        };

        let cfg = match config::load_or_default(&cfg_paths) {
            Ok(cfg) => cfg,
            Err(err) => {
                warn!("config load error: {err:?}");
                config::AppConfig::default()
            }
        };

        let (tx, rx) = mpsc::channel();
        let (mqtt_tx, mqtt_rx) = mpsc::channel();

        let mut keyring_unavailable = false;
        let password = if cfg.mqtt.remember_password {
            match secrets::load_password() {
                Ok(secret) => secret,
                Err(err) => {
                    warn!("keyring load error: {err:?}");
                    keyring_unavailable = true;
                    None
                }
            }
        } else {
            None
        };

        Self {
            cfg_paths,
            cfg,
            password,
            status: String::new(),
            last_save: None,
            keyring_unavailable,
            testing: false,
            test_rx: rx,
            test_tx: tx,
            mqtt_rx,
            mqtt_tx: mqtt_tx.clone(),
            metrics: Metrics::default(),
            connected: false,
            mqtt_handle: None,
            mqtt_stop: None,
        }
    }

    fn save_all(&mut self) {
        let write_cfg = || -> anyhow::Result<()> {
            config::save(&self.cfg_paths, &self.cfg)?;
            if self.cfg.mqtt.remember_password {
                if let Some(secret) = &self.password {
                    secrets::save_password(secret)?;
                }
            } else {
                secrets::delete_password()?;
            }
            Ok(())
        };

        match write_cfg() {
            Ok(_) => {
                self.status = "Saved settings".to_string();
                self.last_save = Some(Instant::now());
            }
            Err(err) => {
                self.status = format!("Save failed: {err:#}");
            }
        }
    }

    fn poll_tests(&mut self) {
        while let Ok(msg) = self.test_rx.try_recv() {
            self.testing = false;
            match msg {
                TestResult::Ok => self.status = "MQTT test succeeded".to_string(),
                TestResult::Err(err) => self.status = format!("MQTT test failed: {err}"),
            }
        }
    }

    fn poll_mqtt(&mut self) {
        while let Ok(ev) = self.mqtt_rx.try_recv() {
            match ev {
                MqttEvent::Connected => {
                    self.status = "MQTT connected".to_string();
                    self.connected = true;
                }
                MqttEvent::Disconnected(err) => {
                    self.status = format!("MQTT disconnected: {err}");
                    self.connected = false;
                    if let Some(handle) = self.mqtt_handle.take() {
                        let _ = handle.join();
                    }
                    self.mqtt_stop = None;
                }
                MqttEvent::Status(msg) => {
                    self.status = msg;
                }
                MqttEvent::Metric { topic, value, kind } => {
                    self.metrics.last_topic = Some(topic);
                    self.metrics.last_update = Some(Instant::now());
                    let slot = match kind.as_str() {
                        "pm1" => &mut self.metrics.pm1,
                        "pm25" | "pm2_5" => &mut self.metrics.pm25,
                        "pm10" => &mut self.metrics.pm10,
                        "tvoc" => &mut self.metrics.tvoc,
                        "co2" => &mut self.metrics.co2,
                        "temp" | "temperature" => &mut self.metrics.temp,
                        "humidity" => &mut self.metrics.humidity,
                        "battery" => &mut self.metrics.battery,
                        _ => continue,
                    };
                    *slot = Some(value);
                }
            }
        }
    }

    fn stop_mqtt(&mut self) {
        if let Some(stop) = self.mqtt_stop.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.mqtt_handle.take() {
            // best-effort join; listener will exit soon after stop signal
            let _ = handle.join();
        }
        self.connected = false;
        self.status = "MQTT stopped".to_string();
    }

    fn forget_password(&mut self) {
        match secrets::delete_password() {
            Ok(_) => {
                self.password = None;
                self.cfg.mqtt.remember_password = false;
                self.status = "Removed saved password".to_string();
            }
            Err(err) => {
                self.status = format!("Could not remove password: {err:#}");
            }
        }
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("MQTT Broker");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Host");
            ui.text_edit_singleline(&mut self.cfg.mqtt.host);
            ui.label("Port");
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.port).clamp_range(1..=65535));
        });

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.cfg.mqtt.tls, "TLS");
            ui.label("CA path");
            let mut ca_str = self
                .cfg
                .mqtt
                .ca_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut ca_str).changed() {
                self.cfg.mqtt.ca_path = if ca_str.trim().is_empty() {
                    None
                } else {
                    Some(ca_str.into())
                };
            }
        });

        ui.horizontal(|ui| {
            ui.label("Client ID");
            let mut cid = self.cfg.mqtt.client_id.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut cid).changed() {
                self.cfg.mqtt.client_id = if cid.trim().is_empty() {
                    None
                } else {
                    Some(cid)
                };
            }
        });

        ui.horizontal(|ui| {
            ui.label("Username");
            let mut uname = self.cfg.mqtt.username.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut uname).changed() {
                self.cfg.mqtt.username = if uname.trim().is_empty() {
                    None
                } else {
                    Some(uname)
                };
            }
        });

        ui.horizontal(|ui| {
            ui.label("Password");
            let mut masked = self.password.clone().unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut masked).password(true))
                .changed()
            {
                self.password = if masked.is_empty() {
                    None
                } else {
                    Some(masked)
                };
            }
        });

        ui.horizontal(|ui| {
            ui.label("Topic prefix");
            let mut prefix = self.cfg.mqtt.topic_prefix.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut prefix).changed() {
                self.cfg.mqtt.topic_prefix = if prefix.trim().is_empty() {
                    None
                } else {
                    Some(prefix)
                };
            }
        });

        ui.horizontal(|ui| {
            ui.label("QoS");
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.qos).clamp_range(0..=2));
            ui.label("Keepalive (s)");
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.keepalive_secs).clamp_range(5..=1200));
        });

        ui.horizontal(|ui| {
            let mut remember = self.cfg.mqtt.remember_password;
            if ui
                .checkbox(&mut remember, "Remember password in system keyring")
                .changed()
            {
                self.cfg.mqtt.remember_password = remember;
                if remember && self.password.is_none() {
                    self.status = "Enter a password to store".to_string();
                }
            }
            if self.keyring_unavailable {
                ui.label(
                    egui::RichText::new("Keyring unavailable; using session-only")
                        .italics()
                        .color(egui::Color32::YELLOW),
                );
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Save settings").clicked() {
                self.save_all();
            }
            if ui
                .add_enabled(!self.testing, egui::Button::new("Test connection"))
                .clicked()
            {
                self.status = "Testing connection...".to_string();
                self.testing = true;
                let cfg = self.cfg.clone();
                let password = self.password.clone();
                let tx = self.test_tx.clone();
                std::thread::spawn(move || {
                    let result = match mqtt::test_connection(&cfg.mqtt, password.as_deref()) {
                        Ok(_) => TestResult::Ok,
                        Err(err) => TestResult::Err(format!("{err:#}")),
                    };
                    let _ = tx.send(result);
                });
            }
            if ui.button("Forget saved password").clicked() {
                self.forget_password();
            }
            if ui
                .add_enabled(self.mqtt_handle.is_none(), egui::Button::new("Start MQTT"))
                .clicked()
            {
                if self.cfg.mqtt.username.is_some() && self.password.is_none() {
                    self.status = "Password required when username is set".to_string();
                    return;
                }
                let cfg = self.cfg.clone();
                let password = self.password.clone();
                let tx = self.mqtt_tx.clone();
                let (stop_tx, stop_rx) = mpsc::channel();
                self.status = "Starting MQTT listener...".to_string();
                let handle = std::thread::spawn(move || {
                    let _ = mqtt::run_listener(cfg.mqtt, password.as_deref(), tx, stop_rx);
                });
                self.mqtt_handle = Some(handle);
                self.mqtt_stop = Some(stop_tx);
            }
            if ui
                .add_enabled(self.mqtt_handle.is_some(), egui::Button::new("Stop MQTT"))
                .clicked()
            {
                self.stop_mqtt();
            }
            if let Some(t) = self.last_save {
                ui.label(format!("Last saved {}s ago", t.elapsed().as_secs()));
            }
        });

        if !self.status.is_empty() {
            ui.separator();
            ui.label(&self.status);
        }
    }
}

impl Drop for Air1App {
    fn drop(&mut self) {
        self.stop_mqtt();
    }
}

impl App for Air1App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_tests();
        self.poll_mqtt();

        // simple modern look
        ctx.set_visuals(egui::Visuals::dark());

        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Air 1 MQTT Monitor (Rust + egui)");
                ui.label(format!("Status: {}", self.status));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::CollapsingHeader::new("Connection Settings")
                .default_open(true)
                .show(ui, |ui| self.draw_settings(ui));

            ui.separator();
            ui.heading("Live dashboard");

            let availability = match (self.connected, self.metrics.last_update) {
                (false, _) => ("offline", egui::Color32::RED),
                (true, Some(ts)) => {
                    let age = ts.elapsed();
                    if age.as_secs() <= 15 {
                        ("fresh", egui::Color32::GREEN)
                    } else if age.as_secs() <= 60 {
                        ("stale", egui::Color32::YELLOW)
                    } else {
                        ("stalled", egui::Color32::RED)
                    }
                }
                (true, None) => ("no data", egui::Color32::YELLOW),
            };

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Connection: {}",
                        if self.connected { "online" } else { "offline" }
                    ))
                    .color(if self.connected {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::RED
                    }),
                );
                ui.label(
                    egui::RichText::new(format!("Availability: {}", availability.0))
                        .color(availability.1),
                );
                if let Some(ts) = self.metrics.last_update {
                    ui.label(format!("Last update: {}s ago", ts.elapsed().as_secs()));
                }
            });

            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                self.metric_card(
                    ui,
                    "PM1",
                    self.metrics.pm1,
                    egui::Color32::from_rgb(86, 156, 214),
                );
                self.metric_card(
                    ui,
                    "PM2.5",
                    self.metrics.pm25,
                    egui::Color32::from_rgb(90, 200, 90),
                );
                self.metric_card(
                    ui,
                    "PM10",
                    self.metrics.pm10,
                    egui::Color32::from_rgb(237, 167, 54),
                );
                self.metric_card(
                    ui,
                    "VOC",
                    self.metrics.tvoc,
                    egui::Color32::from_rgb(180, 130, 255),
                );
                self.metric_card(
                    ui,
                    "CO2",
                    self.metrics.co2,
                    egui::Color32::from_rgb(255, 99, 71),
                );
                self.metric_card(
                    ui,
                    "Temp",
                    self.metrics.temp,
                    egui::Color32::from_rgb(255, 214, 102),
                );
                self.metric_card(
                    ui,
                    "Humidity",
                    self.metrics.humidity,
                    egui::Color32::from_rgb(102, 204, 255),
                );
                self.metric_card(
                    ui,
                    "Battery",
                    self.metrics.battery,
                    egui::Color32::from_rgb(170, 170, 170),
                );
            });

            if let Some(last) = &self.metrics.last_topic {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("Last topic: {last}"))
                        .italics()
                        .color(egui::Color32::GRAY),
                );
            }
        });
    }
}

impl Air1App {
    fn metric_card(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        value: Option<f64>,
        color: egui::Color32,
    ) {
        let text = match value {
            Some(v) => format!("{:.1}", v),
            None => "--".to_string(),
        };
        let card = egui::Frame::none()
            .fill(egui::Color32::from_gray(30))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
            .rounding(egui::Rounding::same(8.0))
            .inner_margin(egui::Margin::symmetric(10.0, 8.0));
        card.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).color(color).size(14.0));
                ui.label(
                    egui::RichText::new(text)
                        .size(20.0)
                        .color(egui::Color32::WHITE),
                );
            });
        });
    }
}
