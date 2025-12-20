use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub ca_path: Option<PathBuf>,
    pub client_id: Option<String>,
    pub username: Option<String>,
    pub topic_prefix: Option<String>,
    pub qos: u8,
    pub keepalive_secs: u16,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub mqtt: MqttConfig,
}

pub struct ConfigPaths {
    pub config_file: PathBuf,
}

impl ConfigPaths {
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

pub fn load_or_default(paths: &ConfigPaths) -> Result<AppConfig> {
    match fs::read_to_string(&paths.config_file) {
        Ok(raw) => {
            let cfg: AppConfig = toml::from_str(&raw).with_context(|| {
                format!("failed to parse config at {}", paths.config_file.display())
            })?;
            Ok(cfg)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(err) => Err(err)
            .with_context(|| format!("failed to read config at {}", paths.config_file.display())),
    }
}

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
