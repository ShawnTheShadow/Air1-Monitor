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
    last_viewport_size: Option<egui::Vec2>,
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
            last_viewport_size: None,
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
            last_viewport_size: None,
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

    fn draw_overall_quality(&self, ui: &mut egui::Ui) {
        // Calculate overall air quality based on PM2.5 primarily
        let (quality_text, quality_color, quality_icon) = if let Some(pm25) = self.metrics.pm25 {
            if pm25 < 12.0 {
                (
                    "Excellent Air Quality",
                    egui::Color32::from_rgb(76, 175, 80),
                    "★",
                )
            } else if pm25 < 35.0 {
                (
                    "Good Air Quality",
                    egui::Color32::from_rgb(139, 195, 74),
                    "●",
                )
            } else if pm25 < 55.0 {
                (
                    "Moderate Air Quality",
                    egui::Color32::from_rgb(255, 235, 59),
                    "◐",
                )
            } else if pm25 < 150.0 {
                (
                    "Poor Air Quality",
                    egui::Color32::from_rgb(255, 152, 0),
                    "▲",
                )
            } else if pm25 < 250.0 {
                (
                    "Unhealthy Air Quality",
                    egui::Color32::from_rgb(244, 67, 54),
                    "⬣",
                )
            } else {
                (
                    "Hazardous Air Quality",
                    egui::Color32::from_rgb(156, 39, 176),
                    "✖",
                )
            }
        } else {
            ("Air Quality Unknown", egui::Color32::GRAY, "?")
        };

        let frame = egui::Frame::default()
            .fill(quality_color.linear_multiply(0.15))
            .stroke(egui::Stroke::new(2.0, quality_color))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(16, 12));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(quality_icon)
                        .size(32.0)
                        .color(quality_color),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(quality_text)
                            .size(22.0)
                            .strong()
                            .color(quality_color),
                    );
                    if let Some(pm25) = self.metrics.pm25 {
                        ui.label(
                            egui::RichText::new(format!("PM2.5: {:.1} μg/m³", pm25))
                                .size(14.0)
                                .color(egui::Color32::LIGHT_GRAY),
                        );
                    }
                });

                // Add warnings for other concerning metrics
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut warnings = Vec::new();

                    if let Some(co2) = self.metrics.co2 {
                        if co2 > 2000.0 {
                            warnings.push(format!("! High CO₂: {:.0} ppm", co2));
                        }
                    }

                    if let Some(tvoc) = self.metrics.tvoc {
                        if tvoc > 2200.0 {
                            warnings.push(format!("! High VOC: {:.0} ppb", tvoc));
                        }
                    }

                    if !warnings.is_empty() {
                        ui.vertical(|ui| {
                            for warning in warnings {
                                ui.label(
                                    egui::RichText::new(warning)
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(255, 152, 0)),
                                );
                            }
                        });
                    }
                });
            });
        });
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("MQTT Broker");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Host");
            ui.text_edit_singleline(&mut self.cfg.mqtt.host);
            ui.label("Port");
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.port).range(1..=65535));
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
                // Auto-save password when remember_password is checked
                if self.cfg.mqtt.remember_password {
                    self.save_all();
                }
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
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.qos).range(0..=2));
            ui.label("Keepalive (s)");
            ui.add(egui::DragValue::new(&mut self.cfg.mqtt.keepalive_secs).range(5..=1200));
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
                } else {
                    // Auto-save when checkbox changes
                    self.save_all();
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

        // Check for viewport size changes and request repaint for smooth resizing
        let current_size = ctx.content_rect().size();
        if let Some(last_size) = self.last_viewport_size {
            if (current_size.x - last_size.x).abs() > 0.1 || (current_size.y - last_size.y).abs() > 0.1 {
                ctx.request_repaint();
            }
        }
        self.last_viewport_size = Some(current_size);

        // simple modern look
        ctx.set_visuals(egui::Visuals::dark());

        egui::TopBottomPanel::top("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Air 1 MQTT Monitor (Rust + egui)");
                ui.label(format!("Status: {}", self.status));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    // Overall Air Quality Indicator
                    if self.connected {
                        self.draw_overall_quality(ui);
                        ui.add_space(8.0);
                    }

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

                    // Air Quality Section
                    ui.heading("Air Quality (Particulate Matter)");
                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        self.gauge_card(
                            ui,
                            "PM2.5",
                            self.metrics.pm25,
                            "μg/m³",
                            &[
                                (0.0, 12.0, "Good"),
                                (12.0, 35.0, "Moderate"),
                                (35.0, 55.0, "Unhealthy (Sensitive)"),
                                (55.0, 150.0, "Unhealthy"),
                                (150.0, 250.0, "Very Unhealthy"),
                            ],
                            250.0,
                        );
                        self.gauge_card(
                            ui,
                            "PM10",
                            self.metrics.pm10,
                            "μg/m³",
                            &[
                                (0.0, 54.0, "Good"),
                                (54.0, 154.0, "Moderate"),
                                (154.0, 254.0, "Unhealthy (Sensitive)"),
                                (254.0, 354.0, "Unhealthy"),
                                (354.0, 424.0, "Very Unhealthy"),
                            ],
                            500.0,
                        );
                        self.gauge_card(
                            ui,
                            "PM1",
                            self.metrics.pm1,
                            "μg/m³",
                            &[
                                (0.0, 10.0, "Good"),
                                (10.0, 25.0, "Moderate"),
                                (25.0, 50.0, "Unhealthy"),
                            ],
                            100.0,
                        );
                    });

                    ui.add_space(12.0);

                    // Gas Sensors Section
                    ui.heading("Gas Sensors");
                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        self.gauge_card(
                            ui,
                            "CO₂",
                            self.metrics.co2,
                            "ppm",
                            &[
                                (0.0, 800.0, "Excellent"),
                                (800.0, 1000.0, "Good"),
                                (1000.0, 1500.0, "Acceptable"),
                                (1500.0, 2000.0, "Poor"),
                                (2000.0, 5000.0, "Bad"),
                            ],
                            5000.0,
                        );
                        self.gauge_card(
                            ui,
                            "TVOC",
                            self.metrics.tvoc,
                            "ppb",
                            &[
                                (0.0, 220.0, "Excellent"),
                                (220.0, 660.0, "Good"),
                                (660.0, 1430.0, "Moderate"),
                                (1430.0, 2200.0, "Poor"),
                                (2200.0, 5500.0, "Unhealthy"),
                            ],
                            5500.0,
                        );
                    });

                    ui.add_space(12.0);

                    // Environment Section
                    ui.heading("Environment");
                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        self.gauge_card(
                            ui,
                            "Temperature",
                            self.metrics.temp,
                            "°F",
                            &[
                                (32.0, 64.0, "Cool"),
                                (64.0, 75.0, "Comfortable"),
                                (75.0, 82.0, "Warm"),
                                (82.0, 104.0, "Hot"),
                            ],
                            104.0,
                        );
                        self.gauge_card(
                            ui,
                            "Humidity",
                            self.metrics.humidity,
                            "%",
                            &[
                                (0.0, 30.0, "Dry"),
                                (30.0, 60.0, "Comfortable"),
                                (60.0, 80.0, "Humid"),
                                (80.0, 100.0, "Very Humid"),
                            ],
                            100.0,
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
        });
    }
}

impl Air1App {
    fn get_quality_color(value: f64, ranges: &[(f64, f64, &'static str)]) -> egui::Color32 {
        // Color scheme: Green -> Yellow -> Orange -> Red -> Purple
        let colors = [
            egui::Color32::from_rgb(76, 175, 80),  // Green - Good
            egui::Color32::from_rgb(255, 235, 59), // Yellow - Moderate
            egui::Color32::from_rgb(255, 152, 0),  // Orange - Unhealthy for Sensitive
            egui::Color32::from_rgb(244, 67, 54),  // Red - Unhealthy
            egui::Color32::from_rgb(156, 39, 176), // Purple - Very Unhealthy
        ];

        for (i, (min, max, _)) in ranges.iter().enumerate() {
            if value >= *min && value < *max {
                return colors.get(i).copied().unwrap_or(egui::Color32::GRAY);
            }
        }

        // If beyond all ranges, use the last color
        colors
            .get(ranges.len() - 1)
            .copied()
            .unwrap_or(egui::Color32::DARK_RED)
    }

    fn get_quality_label(value: f64, ranges: &[(f64, f64, &'static str)]) -> &'static str {
        for (min, max, label) in ranges {
            if value >= *min && value < *max {
                return label;
            }
        }
        ranges
            .last()
            .map(|(_, _, label)| *label)
            .unwrap_or("Extreme")
    }

    fn draw_gauge(
        &self,
        ui: &mut egui::Ui,
        value: f64,
        max_value: f64,
        ranges: &[(f64, f64, &'static str)],
        size: f32,
    ) {
        let (response, painter) =
            ui.allocate_painter(egui::Vec2::new(size, size), egui::Sense::hover());

        let center = response.rect.center();
        let radius = size / 2.0 - 8.0;
        let stroke_width = 12.0;

        // Draw background arc
        let arc_start = std::f32::consts::PI * 0.75;
        let arc_end = std::f32::consts::PI * 2.25;

        // Draw colored segments
        let total_angle = arc_end - arc_start;
        for (min, max, _) in ranges.iter() {
            let start_ratio = (*min / max_value).min(1.0);
            let end_ratio = (*max / max_value).min(1.0);
            let segment_start = arc_start + total_angle * start_ratio as f32;
            let segment_end = arc_start + total_angle * end_ratio as f32;

            let color = Self::get_quality_color(*min, ranges);
            self.draw_arc(
                &painter,
                center,
                radius,
                segment_start,
                segment_end,
                stroke_width,
                color.linear_multiply(0.3),
            );
        }

        // Draw value arc
        let value_ratio = (value / max_value).min(1.0) as f32;
        let value_angle = arc_start + total_angle * value_ratio;
        let value_color = Self::get_quality_color(value, ranges);
        self.draw_arc(
            &painter,
            center,
            radius,
            arc_start,
            value_angle,
            stroke_width,
            value_color,
        );

        // Draw needle
        let needle_length = radius - stroke_width / 2.0;
        let needle_end = center
            + egui::Vec2::new(
                needle_length * value_angle.cos(),
                needle_length * value_angle.sin(),
            );
        painter.line_segment(
            [center, needle_end],
            egui::Stroke::new(3.0, egui::Color32::WHITE),
        );

        // Draw center circle
        painter.circle_filled(center, 6.0, egui::Color32::from_gray(40));
        painter.circle_stroke(center, 6.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
    }

    fn draw_arc(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        width: f32,
        color: egui::Color32,
    ) {
        let segments = 32;
        let angle_step = (end_angle - start_angle) / segments as f32;

        for i in 0..segments {
            let a1 = start_angle + angle_step * i as f32;
            let a2 = start_angle + angle_step * (i + 1) as f32;

            let p1 = center + egui::Vec2::new(radius * a1.cos(), radius * a1.sin());
            let p2 = center + egui::Vec2::new(radius * a2.cos(), radius * a2.sin());

            painter.line_segment([p1, p2], egui::Stroke::new(width, color));
        }
    }

    fn gauge_card(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        value: Option<f64>,
        unit: &str,
        ranges: &[(f64, f64, &'static str)],
        max_value: f64,
    ) {
        let card_width = 200.0;
        let gauge_size = 140.0;

        let card = egui::Frame::default()
            .fill(egui::Color32::from_gray(25))
            .stroke(egui::Stroke::new(2.0, egui::Color32::from_gray(50)))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::same(16));

        card.show(ui, |ui| {
            ui.set_width(card_width);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(label).size(18.0).strong());
                ui.add_space(8.0);

                if let Some(v) = value {
                    self.draw_gauge(ui, v, max_value, ranges, gauge_size);

                    ui.add_space(8.0);

                    let quality_label = Self::get_quality_label(v, ranges);
                    let quality_color = Self::get_quality_color(v, ranges);

                    ui.label(
                        egui::RichText::new(format!("{:.1} {}", v, unit))
                            .size(24.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );

                    ui.label(
                        egui::RichText::new(quality_label)
                            .size(14.0)
                            .color(quality_color),
                    );
                } else {
                    ui.add_space(gauge_size / 2.0 - 20.0);
                    ui.label(
                        egui::RichText::new("No Data")
                            .size(20.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(gauge_size / 2.0 - 20.0);
                }
            });
        });
    }
}
