use std::{collections::HashSet, fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// MQTT connection settings persisted to the config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// MQTT broker hostname or IP address.
    pub host: String,
    /// MQTT broker port.
    pub port: u16,
    /// Enable TLS for broker connection.
    pub tls: bool,
    /// Optional CA certificate path for TLS verification.
    pub ca_path: Option<PathBuf>,
    /// Optional MQTT client ID.
    pub client_id: Option<String>,
    /// Optional MQTT username.
    pub username: Option<String>,
    /// Optional topic prefix used for subscriptions.
    pub topic_prefix: Option<String>,
    /// MQTT QoS level (0-2).
    pub qos: u8,
    /// Keepalive interval in seconds.
    pub keepalive_secs: u16,
    /// Store password in the system keyring if available.
    pub remember_password: bool,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 1883,
            tls: false,
            ca_path: None,
            client_id: Some("air1-monitor".to_string()),
            username: None,
            topic_prefix: None,
            qos: 0,
            keepalive_secs: 30,
            remember_password: false,
        }
    }
}

/// Per-gauge configuration for dashboard rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaugeConfig {
    /// Gauge identifier string (e.g. "pm25").
    pub id: String,
    /// Whether the gauge is shown in the dashboard.
    pub enabled: bool,
}

impl GaugeConfig {
    /// Create an enabled gauge config with the given identifier.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            enabled: true,
        }
    }
}

/// Configuration for a dashboard section and its gauges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSectionConfig {
    /// Section identifier string (e.g. "air_quality").
    pub id: String,
    /// Whether the section is shown in the dashboard.
    pub enabled: bool,
    /// Ordered list of gauges in this section.
    #[serde(default)]
    pub gauges: Vec<GaugeConfig>,
}

impl DashboardSectionConfig {
    /// Create a section with default gauges for the given section id.
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            enabled: true,
            gauges: Self::default_gauges_for(id),
        }
    }

    fn default_gauges_for(section_id: &str) -> Vec<GaugeConfig> {
        match section_id {
            "air_quality" => vec![
                GaugeConfig::new("pm25"),
                GaugeConfig::new("pm10"),
                GaugeConfig::new("pm1"),
            ],
            "gas" => vec![GaugeConfig::new("co2"), GaugeConfig::new("tvoc")],
            "environment" => vec![
                GaugeConfig::new("temperature"),
                GaugeConfig::new("humidity"),
            ],
            _ => vec![],
        }
    }

    /// Normalize gauges by filling defaults and removing duplicates.
    pub fn normalize(&mut self) {
        let defaults = Self::default_gauges_for(&self.id);
        if self.gauges.is_empty() {
            self.gauges = defaults;
            return;
        }

        let mut seen: HashSet<String> = HashSet::new();
        self.gauges.retain(|gauge| seen.insert(gauge.id.clone()));

        for gauge in defaults {
            if !seen.contains(&gauge.id) {
                self.gauges.push(gauge);
            }
        }
    }
}

/// Dashboard layout configuration with ordered sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Ordered list of dashboard sections.
    pub sections: Vec<DashboardSectionConfig>,
}

impl DashboardConfig {
    /// Normalize sections by filling defaults, deduplicating, and recursing.
    pub fn normalize(&mut self) {
        let defaults = Self::default_sections();
        if self.sections.is_empty() {
            self.sections = defaults;
            return;
        }

        let mut seen: HashSet<String> = HashSet::new();
        self.sections
            .retain(|section| seen.insert(section.id.clone()));

        for section in &mut self.sections {
            section.normalize();
        }

        for section in defaults {
            if !seen.contains(&section.id) {
                self.sections.push(section);
            }
        }

        for section in &mut self.sections {
            section.normalize();
        }
    }

    fn default_sections() -> Vec<DashboardSectionConfig> {
        vec![
            DashboardSectionConfig::new("overview"),
            DashboardSectionConfig::new("air_quality"),
            DashboardSectionConfig::new("gas"),
            DashboardSectionConfig::new("environment"),
        ]
    }
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            sections: Self::default_sections(),
        }
    }
}

/// Root application configuration persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// MQTT configuration block.
    pub mqtt: MqttConfig,
    /// Dashboard layout configuration.
    #[serde(default)]
    pub dashboard: DashboardConfig,
}

/// Resolved paths for configuration files.
pub struct ConfigPaths {
    /// Absolute path to the config.toml file.
    pub config_file: PathBuf,
}

impl ConfigPaths {
    /// Resolve the config path using XDG conventions.
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "air1", "monitor")
            .context("could not determine XDG config dir")?;
        let config_dir = dirs.config_dir();
        let config_file = config_dir.join("config.toml");
        Ok(Self { config_file })
    }
}

impl Default for ConfigPaths {
    fn default() -> Self {
        match ConfigPaths::new() {
            Ok(p) => p,
            Err(err) => {
                warn!("ConfigPaths::default fallback: {:#}", err);
                ConfigPaths {
                    config_file: PathBuf::from("config.toml"),
                }
            }
        }
    }
}

/// Load configuration from disk or return defaults if missing.
pub fn load_or_default(paths: &ConfigPaths) -> Result<AppConfig> {
    match fs::read_to_string(&paths.config_file) {
        Ok(raw) => {
            let mut cfg: AppConfig = toml::from_str(&raw).with_context(|| {
                format!("failed to parse config at {}", paths.config_file.display())
            })?;
            cfg.dashboard.normalize();
            if cfg.mqtt.host.trim().is_empty() {
                anyhow::bail!("MQTT host must be non-empty");
            }
            if !(1..=65535).contains(&cfg.mqtt.port) {
                anyhow::bail!("MQTT port must be between 1 and 65535");
            }
            if cfg.mqtt.qos > 2 {
                anyhow::bail!("MQTT QoS must be between 0 and 2");
            }
            if cfg.mqtt.keepalive_secs == 0 {
                anyhow::bail!("MQTT keepalive must be greater than 0 seconds");
            }
            Ok(cfg)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(err) => Err(err)
            .with_context(|| format!("failed to read config at {}", paths.config_file.display())),
    }
}

/// Save configuration to disk with secure permissions.
pub fn save(paths: &ConfigPaths, cfg: &AppConfig) -> Result<()> {
    if let Some(dir) = paths.config_file.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create config dir {}", dir.display()))?;
    }

    let serialized = toml::to_string_pretty(cfg).context("failed to serialize config")?;
    let mut file = fs::File::create(&paths.config_file).with_context(|| {
        format!(
            "failed to open config for write {}",
            paths.config_file.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        file.set_permissions(perms).with_context(|| {
            format!(
                "failed to set permissions on {}",
                paths.config_file.display()
            )
        })?;
    }

    file.write_all(serialized.as_bytes())
        .context("failed to write config")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gauge_ids(section: &DashboardSectionConfig) -> Vec<String> {
        section.gauges.iter().map(|g| g.id.clone()).collect()
    }

    #[test]
    fn section_normalize_fills_defaults_when_empty() {
        let mut section = DashboardSectionConfig {
            id: "gas".to_string(),
            enabled: true,
            gauges: vec![],
        };

        section.normalize();

        let expected = DashboardSectionConfig::default_gauges_for(&section.id);
        let expected_ids: Vec<String> = expected.iter().map(|g| g.id.clone()).collect();
        assert_eq!(section.gauges.len(), expected_ids.len());
        assert_eq!(gauge_ids(&section), expected_ids);
    }

    #[test]
    fn section_normalize_deduplicates_gauges_and_inserts_missing_defaults() {
        let mut section = DashboardSectionConfig {
            id: "air_quality".to_string(),
            enabled: true,
            gauges: vec![
                GaugeConfig::new("pm25"),
                GaugeConfig::new("pm25"),
                GaugeConfig::new("pm10"),
            ],
        };

        section.normalize();

        let ids = gauge_ids(&section);
        let mut set = HashSet::new();
        for id in &ids {
            assert!(set.insert(id.clone()));
        }
        assert!(ids.contains(&"pm25".to_string()));
        assert!(ids.contains(&"pm10".to_string()));
        assert!(ids.contains(&"pm1".to_string()));
    }

    #[test]
    fn section_normalize_unknown_id_has_no_defaults() {
        let mut section = DashboardSectionConfig {
            id: "custom".to_string(),
            enabled: true,
            gauges: vec![],
        };

        section.normalize();

        assert!(section.gauges.is_empty());
        let defaults = DashboardSectionConfig::default_gauges_for(&section.id);
        assert!(defaults.is_empty());
    }

    #[test]
    fn dashboard_normalize_dedupes_sections_and_recurses() {
        let mut dashboard = DashboardConfig {
            sections: vec![
                DashboardSectionConfig {
                    id: "air_quality".to_string(),
                    enabled: true,
                    gauges: vec![GaugeConfig::new("pm25"), GaugeConfig::new("pm25")],
                },
                DashboardSectionConfig {
                    id: "air_quality".to_string(),
                    enabled: false,
                    gauges: vec![GaugeConfig::new("pm10")],
                },
                DashboardSectionConfig {
                    id: "gas".to_string(),
                    enabled: true,
                    gauges: vec![],
                },
                DashboardSectionConfig {
                    id: "custom".to_string(),
                    enabled: true,
                    gauges: vec![GaugeConfig::new("x"), GaugeConfig::new("x")],
                },
            ],
        };

        dashboard.normalize();

        let section_ids: Vec<String> = dashboard.sections.iter().map(|s| s.id.clone()).collect();
        let mut section_set = HashSet::new();
        for id in &section_ids {
            assert!(section_set.insert(id.clone()));
        }
        assert!(section_ids.contains(&"overview".to_string()));
        assert!(section_ids.contains(&"environment".to_string()));

        let air = dashboard
            .sections
            .iter()
            .find(|s| s.id == "air_quality")
            .expect("air_quality section present");
        let air_ids = gauge_ids(air);
        assert!(air_ids.contains(&"pm25".to_string()));
        assert!(air_ids.contains(&"pm10".to_string()));
        assert!(air_ids.contains(&"pm1".to_string()));

        let gas = dashboard
            .sections
            .iter()
            .find(|s| s.id == "gas")
            .expect("gas section present");
        let gas_ids = gauge_ids(gas);
        assert!(gas_ids.contains(&"co2".to_string()));
        assert!(gas_ids.contains(&"tvoc".to_string()));

        let custom = dashboard
            .sections
            .iter()
            .find(|s| s.id == "custom")
            .expect("custom section present");
        let custom_ids = gauge_ids(custom);
        assert_eq!(custom_ids, vec!["x".to_string()]);
    }
}
