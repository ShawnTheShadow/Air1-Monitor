# Air1 Monitor

A modern, GUI-based MQTT monitoring application for Air Quality sensors built with Rust and egui.

![Version](https://img.shields.io/badge/version-0.1.4-blue.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

## Overview

Air1 Monitor is a desktop application designed to work with Home Assistant air quality sensors. It connects to MQTT brokers to monitor air quality sensor data published by Home Assistant automations in real-time. The application provides a clean, intuitive interface for tracking various air quality metrics including particulate matter (PM), temperature, humidity, CO2, and TVOC levels.

## Features

- 📊 **Real-time Monitoring**: Live display of air quality metrics
- 🔒 **Secure Connections**: TLS/SSL support with custom CA certificates
- 🔑 **Password Management**: Secure credential storage using system keyring
- 📡 **MQTT Protocol**: Full MQTT client with customizable topics
- 🎨 **Modern UI**: Clean, responsive interface built with egui
- 📝 **Configuration**: Persistent TOML-based configuration

## Home Assistant Integration

This application is designed to receive air quality data from Home Assistant via MQTT. You'll need to set up Home Assistant automations to publish your sensor data to MQTT topics that this application subscribes to.

### Example Home Assistant Automation

```yaml
automation:
  - alias: "Publish Air Quality to MQTT"
    trigger:
      - platform: state
        entity_id:
          - sensor.air_quality_pm25
          - sensor.air_quality_pm10
          - sensor.air_quality_temperature
          - sensor.air_quality_humidity
          - sensor.air_quality_co2
          - sensor.air_quality_tvoc
    action:
      - service: mqtt.publish
        data:
          topic: "air/sensor/{{ trigger.entity_id.split('.')[1] }}"
          payload: "{{ trigger.to_state.state }}"
```

## Monitored Metrics

- **PM1.0, PM2.5, PM10**: Particulate matter concentrations
- **TVOC**: Total Volatile Organic Compounds
- **CO2**: Carbon Dioxide levels
- **Temperature**: Ambient temperature
- **Humidity**: Relative humidity

## Requirements

- Linux (x86_64)
- GCC libraries
- MQTT broker (local or remote)

## Installation

### From Package (Arch Linux)

```bash
# Install the package
sudo pacman -U air1-monitor-0.1.4.r47-1-x86_64.pkg.tar.zst
```

### From Source

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone the repository
git clone https://github.com/ShawnTheShadow/Air1-Monitor.git
cd Air1-Monitor

# Build the application
cargo build --release

# Run the application
./target/release/air1-monitor
```

### Docker Build

```bash
# Use the included Docker build script
./build-in-docker.sh
```

## Configuration

Configuration is stored in `~/.config/air1-monitor/config.toml`:

```toml
[mqtt]
broker = "mqtt.example.com"
port = 8883
use_tls = true
username = "your_username"
client_id = "air1-monitor"
keepalive_secs = 60
base_topic = "air/sensor"
ca_cert_path = "/path/to/ca.crt"
```

### Password Storage

Passwords are securely stored using the system keyring. On first run, you'll be prompted to enter your MQTT broker password.

## Usage

### Prerequisites

1. Set up Home Assistant with air quality sensors
2. Configure Home Assistant automations to publish sensor data to MQTT (see [Home Assistant Integration](#home-assistant-integration))
3. Ensure your MQTT broker is accessible

### Running the Application

1. Launch the application from your application menu or run `air1-monitor` from the terminal
2. Configure your MQTT broker connection settings (same broker used by Home Assistant)
3. Set the base topic to match your Home Assistant automation topics
4. Enter your password (stored securely in the system keyring)
5. Click "Connect" to start monitoring
6. View real-time metrics in the main window as Home Assistant publishes updates

## Building Packages

### Arch Linux Package

```bash
# Build using makepkg
makepkg -si

# Or build with the provided script
./build-in-docker.sh
```

## Development

### Project Structure

```
air1-monitor/
├── src/
│   ├── main.rs       # Application entry point
│   ├── app.rs        # Main application logic
│   ├── config.rs     # Configuration management
│   ├── mqtt.rs       # MQTT client implementation
│   └── secrets.rs    # Secure credential management
├── build.rs          # Build script for Git versioning
├── Cargo.toml        # Rust dependencies
└── PKGBUILD          # Arch Linux package definition
```

### Dependencies

- **eframe/egui**: Modern GUI framework
- **rumqttc**: MQTT client library
- **rustls**: TLS/SSL implementation
- **keyring**: Secure credential storage
- **serde/toml**: Configuration serialization

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License.

## Author

**ShawnTheShadow** - [contact@stsg.io](mailto:contact@stsg.io)

## Links

- [GitHub Repository](https://github.com/ShawnTheShadow/Air1-Monitor)
- [Issue Tracker](https://github.com/ShawnTheShadow/Air1-Monitor/issues)
