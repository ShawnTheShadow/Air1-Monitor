use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use air1_monitor::{app::Air1App, config};

fn make_unique_tempdir() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = env::temp_dir().join(format!("air1_monitor_tests_{}_{}", std::process::id(), now));
    let _ = fs::create_dir_all(&base);
    base
}

#[test]
fn test_app_initialization_with_explicit_config() {
    // Use a unique temp XDG config dir so `ConfigPaths::new()` is deterministic
    let base = make_unique_tempdir();
    let prev = env::var_os("XDG_CONFIG_HOME");
    unsafe {
        env::set_var("XDG_CONFIG_HOME", &base);
    }

    // Prepare a minimal config with a known keepalive value
    let mut cfg = config::AppConfig::default();
    cfg.mqtt.keepalive_secs = 42;

    let paths = config::ConfigPaths::new().expect("failed to build config paths");
    config::save(&paths, &cfg).expect("failed to write test config");

    let app = Air1App::init();

    // deterministic assertions
    assert!(
        app.status.is_empty(),
        "unexpected status on init: {}",
        app.status
    );
    assert_eq!(app.cfg.mqtt.keepalive_secs, 42);

    // cleanup and restore env
    let _ = fs::remove_dir_all(&base);
    if let Some(prev) = prev {
        unsafe {
            env::set_var("XDG_CONFIG_HOME", prev);
        }
    } else {
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }
    }
}

#[test]
fn test_app_default_status_is_empty() {
    // Test the default constructor produces an empty status
    let app = Air1App::default();
    assert!(
        app.status.is_empty(),
        "default app had non-empty status: {}",
        app.status
    );
}
