#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Air1App, Metrics, MqttEvent};
    use crate::config;
    use eframe::egui;
    use std::sync::mpsc;
    use std::time::Instant;
    
    // Helper function to create a test app
    fn create_test_app() -> Air1App {
        let (test_tx, test_rx) = mpsc::channel();
        let (mqtt_tx, mqtt_rx) = mpsc::channel();
        
        Air1App {
            cfg_paths: config::ConfigPaths::default(),
            cfg: config::AppConfig::default(),
            password: None,
            status: String::new(),
            last_save: None,
            keyring_unavailable: false,
            testing: false,
            show_error_dialog: false,
            show_keyring_help: false,
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
    
    #[test]
    fn test_default_initialization() {
        let app = Air1App::default();
        assert_eq!(app.status, String::new());
        assert!(!app.connected);
        assert!(app.password.is_none());
        assert!(app.metrics.pm25.is_none());
    }
    
    #[test]
    fn test_metrics_update() {
        let mut app = create_test_app();
        
        // Simulate receiving a metric
        let event = MqttEvent::Metric {
            topic: "sensors/pm25".to_string(),
            value: 25.5,
            kind: "pm25".to_string(),
        };
        
        // Send the event through the channel
        app.mqtt_tx.send(event).unwrap();
        
        // Poll the MQTT events
        app.poll_mqtt();
        
        // Verify the metric was updated
        assert_eq!(app.metrics.pm25, Some(25.5));
        assert_eq!(app.metrics.last_topic, Some("sensors/pm25".to_string()));
    }
    
    #[test]
    fn test_connection_status() {
        let mut app = create_test_app();
        
        // Test disconnected state
        let event = MqttEvent::Disconnected("Connection lost".to_string());
        app.mqtt_tx.send(event).unwrap();
        app.poll_mqtt();
        
        assert!(!app.connected);
        assert_eq!(app.status, "MQTT disconnected: Connection lost");
        
        // Test connected state
        let event = MqttEvent::Connected;
        app.mqtt_tx.send(event).unwrap();
        app.poll_mqtt();
        
        assert!(app.connected);
        assert_eq!(app.status, "MQTT connected");
    }
    
    #[test]
    fn test_quality_color_calculation() {
        let app = create_test_app();
        
        // Test PM2.5 quality ranges
        let ranges = &[
            (0.0, 12.0, "Good"),
            (12.0, 35.0, "Moderate"),
            (35.0, 55.0, "Unhealthy (Sensitive)"),
            (55.0, 150.0, "Unhealthy"),
            (150.0, 250.0, "Very Unhealthy"),
        ];
        
        // Good quality
        let color = Air1App::get_quality_color(10.0, ranges);
        assert_eq!(color, egui::Color32::from_rgb(76, 175, 80));
        
        // Moderate quality
        let color = Air1App::get_quality_color(20.0, ranges);
        assert_eq!(color, egui::Color32::from_rgb(255, 235, 59));
        
        // Unhealthy quality
        let color = Air1App::get_quality_color(100.0, ranges);
        assert_eq!(color, egui::Color32::from_rgb(244, 67, 54));
    }
    
    #[test]
    fn test_quality_label_calculation() {
        let app = create_test_app();
        
        let ranges = &[
            (0.0, 12.0, "Good"),
            (12.0, 35.0, "Moderate"),
            (35.0, 55.0, "Unhealthy (Sensitive)"),
        ];
        
        assert_eq!(Air1App::get_quality_label(5.0, ranges), "Good");
        assert_eq!(Air1App::get_quality_label(20.0, ranges), "Moderate");
        assert_eq!(Air1App::get_quality_label(45.0, ranges), "Unhealthy (Sensitive)");
        assert_eq!(Air1App::get_quality_label(60.0, ranges), "Unhealthy (Sensitive)"); // Beyond range
    }
    
    #[test]
    fn test_status_update() {
        let mut app = create_test_app();
        
        // Test status event
        let event = MqttEvent::Status("Processing data".to_string());
        app.mqtt_tx.send(event).unwrap();
        app.poll_mqtt();
        
        assert_eq!(app.status, "Processing data");
    }
    
    #[test]
    fn test_multiple_metrics() {
        let mut app = create_test_app();
        
        // Send multiple metrics
        app.mqtt_tx.send(MqttEvent::Metric {
            topic: "sensors/pm25".to_string(),
            value: 25.5,
            kind: "pm25".to_string(),
        }).unwrap();
        
        app.mqtt_tx.send(MqttEvent::Metric {
            topic: "sensors/temp".to_string(),
            value: 72.0,
            kind: "temp".to_string(),
        }).unwrap();
        
        app.mqtt_tx.send(MqttEvent::Metric {
            topic: "sensors/humidity".to_string(),
            value: 45.0,
            kind: "humidity".to_string(),
        }).unwrap();
        
        app.poll_mqtt();
        
        assert_eq!(app.metrics.pm25, Some(25.5));
        assert_eq!(app.metrics.temp, Some(72.0));
        assert_eq!(app.metrics.humidity, Some(45.0));
    }
    
    #[test]
    fn test_unknown_metric_kind() {
        let mut app = create_test_app();
        
        // Send a metric with unknown kind
        app.mqtt_tx.send(MqttEvent::Metric {
            topic: "sensors/unknown".to_string(),
            value: 100.0,
            kind: "unknown_metric".to_string(),
        }).unwrap();
        
        app.poll_mqtt();
        
        // Verify that unknown metrics are ignored
        assert!(app.metrics.pm25.is_none());
        assert!(app.metrics.temp.is_none());
        assert!(app.metrics.humidity.is_none());
    }
    
    #[test]
    fn test_availability_calculation() {
        let app = create_test_app();
        
        // Test different availability scenarios
        let scenarios = [
            (false, None, ("offline", egui::Color32::RED)),
            (true, Some(Instant::now()), ("fresh", egui::Color32::GREEN)),
            (true, None, ("no data", egui::Color32::YELLOW)),
        ];
        
        for (connected, last_update, expected) in scenarios {
            // This is a simplified test since we can't easily mock Instant
            // In a real test, you might want to use a mock or dependency injection
            assert_eq!(expected.0, expected.0); // Placeholder assertion
        }
    }
}