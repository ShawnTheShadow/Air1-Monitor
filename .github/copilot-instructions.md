# Copilot instructions for Air1 Monitor

## Big picture architecture
- GUI desktop app built with eframe/egui; entry point is [src/main.rs](src/main.rs) which initializes logging, window options, and starts `Air1App::init()`.
- App state + UI live in `Air1App` (see [src/app.rs](src/app.rs)); MQTT runs on a background thread and sends `MqttEvent` messages over `std::sync::mpsc` channels back to the UI.
- MQTT client logic (connect/test/subscribe/reconnect) is in [src/mqtt.rs](src/mqtt.rs); it maps MQTT publishes into `Metric` events via `map_sensor_kind()` (e.g., names ending in `pm_2_5mm_weight_concentration`, containing `co2`, `temperature`, `humidity`).
- Configuration is TOML stored in the XDG config dir via `directories::ProjectDirs::from("com", "air1", "monitor")` (see [src/config.rs](src/config.rs)); config file is `config.toml` under that directory.
- Secrets are stored in the system keyring (service `com.air1.monitor`, account `air1-mqtt`) in [src/secrets.rs](src/secrets.rs); passwords are never stored in the config file.

## Data flow and MQTT conventions
- Subscriptions are derived from `mqtt.topic_prefix`; default base is `homeassistant` and the app subscribes to `{base}/#` after normalizing trailing `/#` (see `subscriptions()` in [src/mqtt.rs](src/mqtt.rs)).
- `Start MQTT` spawns a listener thread and keeps a `stop` channel; `Stop MQTT` sends the stop signal and joins the thread (see [src/app.rs](src/app.rs)).
- Connection test uses `mqtt::test_connection()` (5s timeout) and subscribes to `{prefix}/status` or `homeassistant/status` when no prefix is set.
- TLS uses rustls; if `ca_path` is not set, native certs are loaded (see `tls_config()` in [src/mqtt.rs](src/mqtt.rs)).

## Build, run, test workflows
- Standard Rust workflows: `cargo build`, `cargo build --release`, `cargo test` (documented in [README.md](README.md)).
- `RUST_LOG=debug cargo run` enables tracing output (logger initialized in [src/main.rs](src/main.rs)).
- `build.rs` injects `CARGO_PKG_VERSION_GIT` from `git rev-list --count HEAD` to create version strings like `0.1.4.r123`.
- Arch package builds use `PKGBUILD` / `makepkg -si`; Docker build is via `./build-in-docker.sh` (see [README.md](README.md)).
- Keyring tests skip on CI or when `SKIP_KEYRING_TESTS` is set (see tests in [src/secrets.rs](src/secrets.rs)).

## Project-specific coding patterns
- Error handling uses `anyhow::Result` and `Context` (see [src/config.rs](src/config.rs), [src/mqtt.rs](src/mqtt.rs), [src/secrets.rs](src/secrets.rs)); avoid panics in app code.
- UI is immediate-mode; keep state in `Air1App` and update it from `MqttEvent` in `poll_mqtt()` (see [src/app.rs](src/app.rs)).
- Config writes happen after keyring operations succeed; when keyring fails, `remember_password` is cleared and UI shows a warning (see `save_all()` in [src/app.rs](src/app.rs)).
- Tests that depend on config paths use a temporary `XDG_CONFIG_HOME` for determinism (see [tests/integration_test.rs](tests/integration_test.rs)).

## Reference map
- GUI + app state: [src/app.rs](src/app.rs)
- MQTT client + mapping: [src/mqtt.rs](src/mqtt.rs)
- Config + persistence: [src/config.rs](src/config.rs)
- Keyring secrets: [src/secrets.rs](src/secrets.rs)
- Versioning build script: [build.rs](build.rs)
