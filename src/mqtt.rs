use std::{
    fs,
    net::ToSocketAddrs,
    path::Path,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rumqttc::tokio_rustls::rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, pem::PemObject},
};
use rumqttc::{Client, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use tracing::error;

use crate::config::MqttConfig;

/// Test a one-shot MQTT connection and subscribe to a status topic.
pub fn test_connection(cfg: &MqttConfig, password: Option<&str>) -> Result<()> {
    socket_check(cfg)?;

    let mut opts = build_options(cfg, password)?;
    opts.set_keep_alive(Duration::from_secs(cfg.keepalive_secs.into()));
    opts.set_clean_session(true);

    let (client, mut connection) = Client::new(opts, 10);
    client.subscribe(test_topic(cfg), QoS::AtMostOnce)?;

    let start = Instant::now();
    let timeout = Duration::from_secs(5);

    for notification in connection.iter() {
        if start.elapsed() > timeout {
            anyhow::bail!("MQTT test timed out after {:?}", timeout);
        }
        match notification {
            Ok(Event::Incoming(Packet::ConnAck(_))) => return Ok(()),
            Ok(_) => continue,
            Err(err) => return Err(err).context("MQTT error during test"),
        }
    }

    anyhow::bail!("MQTT test ended without ConnAck")
}

/// Run the MQTT listener loop and forward events to the UI thread.
pub fn run_listener(
    cfg: MqttConfig,
    password: Option<&str>,
    tx: std::sync::mpsc::Sender<crate::app::MqttEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let mut backoff = Duration::from_secs(1);

    loop {
        if stop_rx.try_recv().is_ok() {
            let _ = tx.send(crate::app::MqttEvent::Status(
                "MQTT stop requested".to_string(),
            ));
            let _ = tx.send(crate::app::MqttEvent::Disconnected("stopped".to_string()));
            break;
        }

        let mut opts = build_options(&cfg, password)?;
        opts.set_clean_session(false);
        let (client, mut connection) = Client::new(opts, 20);
        let connect_at = Instant::now();

        let subs = subscriptions(&cfg);
        for sub in &subs {
            client.subscribe(sub.clone(), QoS::AtMostOnce)?;
        }
        let _ = tx.send(crate::app::MqttEvent::Status(format!(
            "MQTT connected; subs: {}",
            subs.join(", ")
        )));
        let _ = tx.send(crate::app::MqttEvent::Connected);

        let mut stopped = false;
        let mut disconnect_reason: Option<String> = None;
        for notification in connection.iter() {
            if stop_rx.try_recv().is_ok() {
                stopped = true;
                let _ = tx.send(crate::app::MqttEvent::Status(
                    "MQTT stop requested".to_string(),
                ));
                let _ = client.disconnect();
                break;
            }
            match notification {
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    if let Some(evt) = map_publish(&p) {
                        let _ = tx.send(evt);
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    error!("MQTT connection error: {:#}", err);
                    disconnect_reason = Some(format!("{err:#}"));
                    break;
                }
            }
        }

        if stopped {
            let _ = tx.send(crate::app::MqttEvent::Disconnected("stopped".to_string()));
            break;
        }

        let reason = disconnect_reason.unwrap_or_else(|| "connection closed".to_string());
        let _ = tx.send(crate::app::MqttEvent::Disconnected(reason));

        if connect_at.elapsed() >= Duration::from_secs(60) {
            backoff = Duration::from_secs(1);
        }

        let wait = backoff;
        let _ = tx.send(crate::app::MqttEvent::Status(format!(
            "Reconnecting in {}s",
            wait.as_secs()
        )));
        if stop_rx.recv_timeout(wait).is_ok() {
            let _ = tx.send(crate::app::MqttEvent::Status(
                "MQTT stop requested".to_string(),
            ));
            let _ = tx.send(crate::app::MqttEvent::Disconnected("stopped".to_string()));
            break;
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }

    Ok(())
}

fn build_options(cfg: &MqttConfig, password: Option<&str>) -> Result<MqttOptions> {
    let client_id = cfg
        .client_id
        .clone()
        .unwrap_or_else(|| "air1-monitor".to_string());
    let mut opts = MqttOptions::new(client_id, cfg.host.clone(), cfg.port);
    if let Some(user) = cfg.username.as_deref() {
        opts.set_credentials(user, password.unwrap_or(""));
    }
    opts.set_keep_alive(Duration::from_secs(cfg.keepalive_secs.into()));
    if cfg.tls {
        let tls = tls_config(cfg.ca_path.as_deref())?;
        opts.set_transport(Transport::tls_with_config(tls));
    }
    Ok(opts)
}

fn subscriptions(cfg: &MqttConfig) -> Vec<String> {
    let raw = cfg
        .topic_prefix
        .as_deref()
        .unwrap_or("homeassistant")
        .trim();

    // Normalize: strip any trailing wildcard the user may have entered (e.g., "apollo_air1/#")
    // and collapse trailing slashes.
    let base = raw
        .trim_end_matches("/#")
        .trim_end_matches('#')
        .trim_end_matches('/');

    vec![format!("{base}/#")]
}

fn tls_config(ca_path: Option<&Path>) -> Result<TlsConfiguration> {
    let mut roots = RootCertStore::empty();
    if let Some(path) = ca_path {
        let data = fs::read(path)
            .with_context(|| format!("failed to read CA file at {}", path.display()))?;
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&data)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| anyhow::anyhow!("failed to parse CA certs"))?;
        let (added, _skipped) = roots.add_parsable_certificates(certs);
        if added == 0 {
            anyhow::bail!("no CA certs added from {}", path.display());
        }
    } else {
        let native_result = rustls_native_certs::load_native_certs();
        if !native_result.errors.is_empty() && native_result.certs.is_empty() {
            anyhow::bail!("failed to load native certs: {:?}", native_result.errors);
        }
        let (added, _skipped) = roots.add_parsable_certificates(native_result.certs);
        if added == 0 {
            anyhow::bail!("no native certificates available");
        }
    }
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConfiguration::Rustls(Arc::new(config)))
}

fn test_topic(cfg: &MqttConfig) -> String {
    if let Some(prefix) = &cfg.topic_prefix {
        format!("{}/status", prefix)
    } else {
        "homeassistant/status".to_string()
    }
}

fn map_publish(p: &rumqttc::Publish) -> Option<crate::app::MqttEvent> {
    let topic = p.topic.clone();
    let payload = String::from_utf8_lossy(&p.payload).trim().to_string();
    let segments = topic.split('/').collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    let name = *segments.last()?; // sensor name is last path component
    let kind = map_sensor_kind(name)?;
    let value: f64 = payload.parse().ok()?;
    Some(crate::app::MqttEvent::Metric {
        topic,
        value,
        kind: kind.to_string(),
    })
}

fn map_sensor_kind(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if n.ends_with("pm_1mm_weight_concentration") {
        Some("pm1")
    } else if n.ends_with("pm_2_5mm_weight_concentration") {
        Some("pm25")
    } else if n.ends_with("pm_10mm_weight_concentration") {
        Some("pm10")
    } else if n.contains("pm_1_to_2_5") {
        Some("pm25")
    } else if n.contains("pm_0_3_to_1") {
        Some("pm1")
    } else if n.contains("pm_2_5_to_4") {
        Some("pm25")
    } else if n.contains("pm_4_to_10") {
        Some("pm10")
    } else if n.contains("voc") || n.contains("sen55_voc") {
        Some("tvoc")
    } else if n.contains("co2") {
        Some("co2")
    } else if n.contains("temp") || n.contains("temperature") {
        Some("temp")
    } else if n.contains("humidity") || n.contains("hum") || n.contains("sen55_humidity") {
        Some("humidity")
    } else {
        None
    }
}

fn socket_check(cfg: &MqttConfig) -> Result<()> {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let mut addrs = addr.to_socket_addrs().context("invalid host/port")?;
    let target = addrs
        .next()
        .context("could not resolve host for socket check")?;
    let timeout = Duration::from_secs(3);
    std::net::TcpStream::connect_timeout(&target, timeout)
        .with_context(|| format!("failed to reach {}", target))?;
    Ok(())
}
