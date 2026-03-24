use std::{sync::mpsc, thread::JoinHandle, time::Instant};
use tracing::warn;

use crate::{config, mqtt, secrets};

pub enum TestResult {
    Ok,
    Err(String),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MqttState {
    #[default]
    Stopped,
    Starting,
    Connected,
    Reconnecting,
    Stopping,
}

impl MqttState {
    pub fn is_running(self) -> bool {
        !matches!(self, MqttState::Stopped)
    }
}

#[derive(Default, Clone, Debug)]
pub struct Metrics {
    pub pm1: Option<f64>,
    pub pm25: Option<f64>,
    pub pm10: Option<f64>,
    pub tvoc: Option<f64>,
    pub co2: Option<f64>,
    pub temp: Option<f64>,
    pub humidity: Option<f64>,
    pub last_topic: Option<String>,
    pub last_update: Option<Instant>,
}

/// Events emitted by the MQTT background thread.
pub enum MqttEvent {
    /// MQTT connection established.
    Connected,
    /// MQTT connection closed with a reason.
    Disconnected(String),
    /// A metric payload mapped to a known sensor kind.
    Metric {
        /// Full MQTT topic for the metric.
        topic: String,
        /// Parsed numeric value.
        value: f64,
        /// Normalized metric kind (e.g. "pm25").
        kind: String,
    },
    /// Human-readable status update.
    Status(String),
}

/// Main application state and UI controller.
pub struct Air1App {
    /// Resolved config file paths.
    pub cfg_paths: config::ConfigPaths,
    /// Current application configuration.
    pub cfg: config::AppConfig,
    /// In-memory MQTT password (optional).
    pub password: Option<String>,
    /// Status message displayed in the UI.
    pub status: String,
    /// Timestamp of the last successful save.
    pub last_save: Option<Instant>,
    /// Whether the system keyring is available.
    pub keyring_unavailable: bool,
    /// Whether an MQTT connection test is running.
    pub testing: bool,
    pub test_rx: mpsc::Receiver<TestResult>,
    pub test_tx: mpsc::Sender<TestResult>,
    pub mqtt_rx: mpsc::Receiver<MqttEvent>,
    pub mqtt_tx: mpsc::Sender<MqttEvent>,
    pub metrics: Metrics,
    pub mqtt_state: MqttState,
    pub connected: bool,
    pub mqtt_handle: Option<JoinHandle<()>>,
    pub mqtt_stop: Option<mpsc::Sender<()>>,
}

#[derive(Copy, Clone)]
struct DashboardSectionDef {
    id: &'static str,
    title: &'static str,
}

const DASHBOARD_SECTIONS: &[DashboardSectionDef] = &[
    DashboardSectionDef {
        id: "overview",
        title: "Overview & Controls",
    },
    DashboardSectionDef {
        id: "air_quality",
        title: "Air Quality (Particulate Matter)",
    },
    DashboardSectionDef {
        id: "gas",
        title: "Gas Sensors",
    },
    DashboardSectionDef {
        id: "environment",
        title: "Environment",
    },
];

// ── Section-to-gauges mapping (used by layout editor) ─────────────────────────

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
            mqtt_state: MqttState::Stopped,
            connected: false,
            mqtt_handle: None,
            mqtt_stop: None,
        }
    }
}

impl Air1App {
    /// Initialize the application state and load configuration.
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

        let mut keyring_unavailable = !secrets::keyring_available();
        let mut status = String::new();

        let password = if cfg.mqtt.remember_password && !keyring_unavailable {
            match secrets::load_password() {
                Ok(secret) => {
                    if secret.is_none() {
                        status = "Password not found in keyring".to_string();
                    }
                    secret
                }
                Err(err) => {
                    warn!("keyring load error: {err:?}");
                    keyring_unavailable = true;
                    status = format!("Keyring error: {err:#}");
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
            status,
            last_save: None,
            keyring_unavailable,
            testing: false,
            test_rx: rx,
            test_tx: tx,
            mqtt_rx,
            mqtt_tx: mqtt_tx.clone(),
            metrics: Metrics::default(),
            mqtt_state: MqttState::Stopped,
            connected: false,
            mqtt_handle: None,
            mqtt_stop: None,
        }
    }

    pub fn save_all(&mut self) {
        let write_cfg = || -> anyhow::Result<()> {
            // Try to save password to keyring first if needed
            if self.cfg.mqtt.remember_password {
                if let Some(secret) = &self.password {
                    secrets::save_password(secret)?;
                }
            } else {
                secrets::delete_password()?;
            }
            // Only save config after keyring operations succeed
            config::save(&self.cfg_paths, &self.cfg)?;
            Ok(())
        };

        match write_cfg() {
            Ok(_) => {
                self.status = "Saved settings".to_string();
                self.last_save = Some(Instant::now());
            }
            Err(err) => {
                // Check if this is a keyring error
                let err_str = format!("{err:#}");
                if err_str.contains("keyring") {
                    warn!("Keyring save failed: {err:#}");
                    self.keyring_unavailable = true;
                    self.cfg.mqtt.remember_password = false;
                    self.status = format!("Keyring error: {err:#}");
                } else {
                    warn!("Settings save failed: {err:#}");
                    self.status = format!("Save failed: {err:#}");
                }
            }
        }
    }

    pub fn poll_tests(&mut self) {
        while let Ok(msg) = self.test_rx.try_recv() {
            self.testing = false;
            match msg {
                TestResult::Ok => self.status = "MQTT test succeeded".to_string(),
                TestResult::Err(err) => self.status = format!("MQTT test failed: {err}"),
            }
        }
    }

    pub fn poll_mqtt(&mut self) {
        while let Ok(ev) = self.mqtt_rx.try_recv() {
            match ev {
                MqttEvent::Connected => {
                    self.status = "MQTT connected".to_string();
                    self.connected = true;
                    self.mqtt_state = MqttState::Connected;
                }
                MqttEvent::Disconnected(err) => {
                    self.status = format!("MQTT disconnected: {err}");
                    self.connected = false;

                    // The listener emits Disconnected on transient failures too.
                    // Joining here would freeze the UI while reconnect loop runs.
                    let listener_finished = self
                        .mqtt_handle
                        .as_ref()
                        .is_some_and(std::thread::JoinHandle::is_finished);
                    if listener_finished {
                        if let Some(handle) = self.mqtt_handle.take() {
                            let _ = handle.join();
                        }
                        self.mqtt_stop = None;
                        self.mqtt_state = MqttState::Stopped;
                    } else {
                        self.mqtt_state = MqttState::Reconnecting;
                    }
                }
                MqttEvent::Status(msg) => {
                    if msg == "MQTT stop requested" {
                        self.mqtt_state = MqttState::Stopping;
                    } else if msg.starts_with("Reconnecting in ") {
                        self.mqtt_state = MqttState::Reconnecting;
                    } else if msg.starts_with("MQTT connected; subs:") {
                        self.mqtt_state = MqttState::Connected;
                    }
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

    pub fn stop_mqtt(&mut self) {
        self.mqtt_state = MqttState::Stopping;
        if let Some(stop) = self.mqtt_stop.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.mqtt_handle.take() {
            // best-effort join; listener will exit soon after stop signal
            let _ = handle.join();
        }
        self.connected = false;
        self.mqtt_state = MqttState::Stopped;
        self.status = "MQTT stopped".to_string();
    }

    pub fn forget_password(&mut self) {
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

    pub fn section_title(id: &str) -> String {
        DASHBOARD_SECTIONS
            .iter()
            .find(|section| section.id == id)
            .map(|section| section.title.to_string())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn gauge_label(id: &str) -> String {
        match id {
            "pm25" => "PM2.5".into(),
            "pm10" => "PM10".into(),
            "pm1" => "PM1".into(),
            "co2" => "CO\u{2082}".into(),
            "tvoc" => "TVOC".into(),
            "temperature" => "Temperature".into(),
            "humidity" => "Humidity".into(),
            other => other.to_string(),
        }
    }

    /// Start the MQTT listener thread. Returns `false` if a password is required but missing.
    pub fn start_mqtt(&mut self) -> bool {
        if self.cfg.mqtt.username.is_some() && self.password.is_none() {
            self.status = "Password required when username is set".to_string();
            return false;
        }
        let cfg = self.cfg.clone();
        let password = self.password.clone();
        let tx = self.mqtt_tx.clone();
        let (stop_tx, stop_rx) = mpsc::channel();
        self.status = "Starting MQTT listener...".to_string();
        self.mqtt_state = MqttState::Starting;
        self.connected = false;
        let handle = std::thread::spawn(move || {
            let _ = mqtt::run_listener(cfg.mqtt, password.as_deref(), tx, stop_rx);
        });
        self.mqtt_handle = Some(handle);
        self.mqtt_stop = Some(stop_tx);
        true
    }

    /// Spawn an ephemeral connection-test thread.
    pub fn spawn_test_connection(&mut self) {
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
}

impl Drop for Air1App {
    fn drop(&mut self) {
        self.stop_mqtt();
    }
}

impl Air1App {
    /// Return the 0-based quality-tier index for a value (clamped to last tier).
    pub fn quality_index(value: f64, ranges: &[(f64, f64, &'static str)]) -> usize {
        for (i, (min, max, _)) in ranges.iter().enumerate() {
            if value >= *min && value < *max {
                return i;
            }
        }
        ranges.len().saturating_sub(1)
    }

    /// Map a value to a quality color as an RGB tuple `(r, g, b)`.
    pub fn get_quality_color(value: f64, ranges: &[(f64, f64, &'static str)]) -> (u8, u8, u8) {
        const COLORS: [(u8, u8, u8); 5] = [
            (76, 175, 80),  // Green  – Good
            (255, 235, 59), // Yellow – Moderate
            (255, 152, 0),  // Orange – Unhealthy for Sensitive
            (244, 67, 54),  // Red    – Unhealthy
            (156, 39, 176), // Purple – Very Unhealthy
        ];
        for (i, (min, max, _)) in ranges.iter().enumerate() {
            if value >= *min && value < *max {
                return COLORS.get(i).copied().unwrap_or((128, 128, 128));
            }
        }
        COLORS
            .get(ranges.len().saturating_sub(1))
            .copied()
            .unwrap_or((139, 0, 0))
    }

    /// Return the quality label string for a value within the provided ranges.
    pub fn get_quality_label(value: f64, ranges: &[(f64, f64, &'static str)]) -> &'static str {
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
}
