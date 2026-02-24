use gtk4::cairo;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use std::{cell::Cell, cell::RefCell, f64::consts::PI, rc::Rc, time::Duration};

use crate::app::Air1App;
use crate::config;

// ── Quality ranges ─────────────────────────────────────────────────────────────

const PM25_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 12.0, "Good"),
    (12.0, 35.0, "Moderate"),
    (35.0, 55.0, "Unhealthy (Sensitive)"),
    (55.0, 150.0, "Unhealthy"),
    (150.0, 250.0, "Very Unhealthy"),
];
const PM10_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 54.0, "Good"),
    (54.0, 154.0, "Moderate"),
    (154.0, 254.0, "Unhealthy (Sensitive)"),
    (254.0, 354.0, "Unhealthy"),
    (354.0, 424.0, "Very Unhealthy"),
];
const PM1_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 10.0, "Good"),
    (10.0, 25.0, "Moderate"),
    (25.0, 50.0, "Unhealthy"),
];
const CO2_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 800.0, "Excellent"),
    (800.0, 1000.0, "Good"),
    (1000.0, 1500.0, "Acceptable"),
    (1500.0, 2000.0, "Poor"),
    (2000.0, 5000.0, "Bad"),
];
const TVOC_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 220.0, "Excellent"),
    (220.0, 660.0, "Good"),
    (660.0, 1430.0, "Moderate"),
    (1430.0, 2200.0, "Poor"),
    (2200.0, 5500.0, "Unhealthy"),
];
const TEMP_RANGES: &[(f64, f64, &str)] = &[
    (32.0, 64.0, "Cool"),
    (64.0, 75.0, "Comfortable"),
    (75.0, 82.0, "Warm"),
    (82.0, 104.0, "Hot"),
];
const HUMIDITY_RANGES: &[(f64, f64, &str)] = &[
    (0.0, 30.0, "Dry"),
    (30.0, 60.0, "Comfortable"),
    (60.0, 80.0, "Humid"),
    (80.0, 100.0, "Very Humid"),
];

// ── CSS ────────────────────────────────────────────────────────────────────────

const CSS: &str = r#"
.quality-0 { color: rgb(76, 175, 80); }
.quality-1 { color: rgb(255, 235, 59); }
.quality-2 { color: rgb(255, 152, 0); }
.quality-3 { color: rgb(244, 67, 54); }
.quality-4 { color: rgb(156, 39, 176); }
.quality-none { color: rgb(150, 150, 150); }



.metric-value { font-size: 22px; font-weight: bold; }
.metric-name  { font-size: 14px; font-weight: bold; }

.banner-good      { background-color: rgba( 76,175, 80,0.22); border: 2px solid rgb( 76,175, 80); border-radius:6px; padding:8px; }
.banner-moderate  { background-color: rgba(255,235, 59,0.22); border: 2px solid rgb(255,235, 59); border-radius:6px; padding:8px; }
.banner-usg       { background-color: rgba(255,152,  0,0.22); border: 2px solid rgb(255,152,  0); border-radius:6px; padding:8px; }
.banner-unhealthy { background-color: rgba(244, 67, 54,0.22); border: 2px solid rgb(244, 67, 54); border-radius:6px; padding:8px; }
.banner-vunhealthy{ background-color: rgba(156, 39,176,0.22); border: 2px solid rgb(156, 39,176); border-radius:6px; padding:8px; }
.banner-unknown   { background-color: rgba(100,100,100,0.22); border: 2px solid rgb(100,100,100); border-radius:6px; padding:8px; }

.connection-online  { color: rgb( 76,175, 80); }
.connection-offline { color: rgb(244, 67, 54); }
.avail-fresh   { color: rgb( 76,175, 80); }
.avail-stale   { color: rgb(255,235, 59); }
.avail-stalled { color: rgb(244, 67, 54); }
.avail-nodata  { color: rgb(255,235, 59); }
.avail-offline { color: rgb(150,150,150); }

.warn-label { color: rgb(255,152,0); font-size: 12px; }
.last-topic { color: rgb(150,150,150); font-style: italic; }
"#;

// ── Widget handles ─────────────────────────────────────────────────────────────

struct GaugeWidgets {
    card: gtk4::Box,
    data_box: gtk4::Box,
    no_data_box: gtk4::Box,
    drawing_area: gtk4::DrawingArea,
    current_value: Rc<Cell<Option<f64>>>,
    value_label: gtk4::Label,
    quality_label: gtk4::Label,
    metric_id: &'static str,
    ranges: &'static [(f64, f64, &'static str)],
}

struct SectionWidget {
    id: String,
    widget: gtk4::Widget,
}

struct AppWidgets {
    status_label: gtk4::Label,
    details_button: gtk4::Button,
    start_button: gtk4::Button,
    stop_button: gtk4::Button,
    connection_label: gtk4::Label,
    availability_label: gtk4::Label,
    last_update_label: gtk4::Label,
    overall_quality_box: gtk4::Box,
    overall_quality_label: gtk4::Label,
    overall_quality_pm25: gtk4::Label,
    overall_warnings: gtk4::Label,
    last_topic_label: gtk4::Label,
    sections: Vec<SectionWidget>,
    gauges: Vec<GaugeWidgets>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn build_ui(gtk_app: &Application, state: Rc<RefCell<Air1App>>, title: &str) {
    // Load CSS
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("No display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = ApplicationWindow::builder()
        .application(gtk_app)
        .title(title)
        .default_width(1280)
        .default_height(900)
        .build();

    let widgets = Rc::new(build_window_contents(&window, state.clone()));

    // 100 ms polling timer
    let state_tick = state.clone();
    let widgets_tick = widgets.clone();
    glib::timeout_add_local(Duration::from_millis(100), move || {
        {
            let mut app = state_tick.borrow_mut();
            app.poll_mqtt();
            app.poll_tests();
        }
        update_widgets(&state_tick.borrow(), &widgets_tick);
        glib::ControlFlow::Continue
    });

    window.present();
}

// ── Window layout ─────────────────────────────────────────────────────────────

fn build_window_contents(window: &ApplicationWindow, state: Rc<RefCell<Air1App>>) -> AppWidgets {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    window.set_child(Some(&root));

    // ── Header bar ───────────────────────────────────────────────────────────
    let header = gtk4::HeaderBar::new();
    window.set_titlebar(Some(&header));

    // Menu button → Configuration / Edit Layout
    let menu_model = gtk4::gio::Menu::new();
    let view_section = gtk4::gio::Menu::new();
    view_section.append(Some("Configuration"), Some("win.show-config"));
    view_section.append(Some("Edit Layout"), Some("win.show-layout"));
    menu_model.append_section(Some("View"), &view_section);

    let menu_btn = gtk4::MenuButton::builder()
        .label("Menu")
        .menu_model(&menu_model)
        .build();
    header.pack_start(&menu_btn);

    // Status row in header
    let status_label = gtk4::Label::new(Some(""));
    header.set_title_widget(Some(&status_label));

    let details_button = gtk4::Button::with_label("Details");
    details_button.set_sensitive(false);
    header.pack_end(&details_button);

    // ── Scrollable main area ──────────────────────────────────────────────────
    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();
    root.append(&scroll);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    scroll.set_child(Some(&content));

    // ── Dashboard sections ────────────────────────────────────────────────────
    let mut sections_vec: Vec<SectionWidget> = Vec::new();
    let mut gauges_vec: Vec<GaugeWidgets> = Vec::new();

    // Overview section
    let (
        overview_frame,
        start_btn,
        stop_btn,
        conn_label,
        avail_label,
        update_label,
        quality_box,
        quality_lbl,
        quality_pm25,
        quality_warn,
    ) = build_overview_section(state.clone(), window);
    content.append(&overview_frame);
    sections_vec.push(SectionWidget {
        id: "overview".into(),
        widget: overview_frame.upcast(),
    });

    // Air Quality section
    let (aq_frame, mut aq_gauges) = build_metric_section(
        "Air Quality (Particulate Matter)",
        &[
            ("pm25", "PM2.5", "μg/m³", PM25_RANGES, 250.0),
            ("pm10", "PM10", "μg/m³", PM10_RANGES, 500.0),
            ("pm1", "PM1", "μg/m³", PM1_RANGES, 100.0),
        ],
    );
    content.append(&aq_frame);
    sections_vec.push(SectionWidget {
        id: "air_quality".into(),
        widget: aq_frame.upcast(),
    });
    gauges_vec.append(&mut aq_gauges);

    // Gas section
    let (gas_frame, mut gas_gauges) = build_metric_section(
        "Gas Sensors",
        &[
            ("co2", "CO₂", "ppm", CO2_RANGES, 5000.0),
            ("tvoc", "TVOC", "ppb", TVOC_RANGES, 5500.0),
        ],
    );
    content.append(&gas_frame);
    sections_vec.push(SectionWidget {
        id: "gas".into(),
        widget: gas_frame.upcast(),
    });
    gauges_vec.append(&mut gas_gauges);

    // Environment section with last-topic label
    let (env_frame, mut env_gauges, last_topic_lbl) = build_environment_section();
    content.append(&env_frame);
    sections_vec.push(SectionWidget {
        id: "environment".into(),
        widget: env_frame.upcast(),
    });
    gauges_vec.append(&mut env_gauges);

    // ── Actions for menu items ────────────────────────────────────────────────
    let show_config_action = gtk4::gio::SimpleAction::new("show-config", None);
    {
        let state_c = state.clone();
        let win_c: gtk4::Window = window.clone().upcast();
        show_config_action.connect_activate(move |_, _| {
            show_config_window(state_c.clone(), &win_c);
        });
    }
    window.add_action(&show_config_action);

    let show_layout_action = gtk4::gio::SimpleAction::new("show-layout", None);
    {
        let state_c = state.clone();
        let win_c: gtk4::Window = window.clone().upcast();
        // We need sections_vec info inside action; pass a clone of section ids
        let section_ids: Vec<String> = sections_vec.iter().map(|s| s.id.clone()).collect();
        show_layout_action.connect_activate(move |_, _| {
            show_layout_window(state_c.clone(), &win_c);
        });
        let _ = section_ids; // suppress unused warning
    }
    window.add_action(&show_layout_action);

    // ── Details button action ─────────────────────────────────────────────────
    {
        let state_c = state.clone();
        let win_c: gtk4::Window = window.clone().upcast();
        details_button.connect_clicked(move |_| {
            let msg = state_c.borrow().status.clone();
            let dlg = gtk4::MessageDialog::builder()
                .transient_for(&win_c)
                .modal(true)
                .message_type(gtk4::MessageType::Info)
                .buttons(gtk4::ButtonsType::Close)
                .text("Status Details")
                .secondary_text(&msg)
                .build();
            dlg.connect_response(|d, _| d.close());
            dlg.present();
        });
    }

    AppWidgets {
        status_label,
        details_button,
        start_button: start_btn,
        stop_button: stop_btn,
        connection_label: conn_label,
        availability_label: avail_label,
        last_update_label: update_label,
        overall_quality_box: quality_box,
        overall_quality_label: quality_lbl,
        overall_quality_pm25: quality_pm25,
        overall_warnings: quality_warn,
        last_topic_label: last_topic_lbl,
        sections: sections_vec,
        gauges: gauges_vec,
    }
}

// ── Section builders ──────────────────────────────────────────────────────────

/// `(metric_id, label, unit, ranges, max_value)` descriptor for a gauge card.
type GaugeSpec = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(f64, f64, &'static str)],
    f64,
);

#[allow(clippy::too_many_arguments)]
fn build_overview_section(
    state: Rc<RefCell<Air1App>>,
    window: &ApplicationWindow,
) -> (
    gtk4::Frame,
    gtk4::Button,
    gtk4::Button,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
    gtk4::Box,
    gtk4::Label,
    gtk4::Label,
    gtk4::Label,
) {
    let frame = gtk4::Frame::new(Some("Overview & Controls"));
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    vbox.set_margin_top(8);
    vbox.set_margin_bottom(8);
    vbox.set_margin_start(8);
    vbox.set_margin_end(8);
    frame.set_child(Some(&vbox));

    // Overall quality banner
    let quality_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    quality_box.add_css_class("banner-unknown");
    quality_box.set_margin_bottom(6);

    let quality_label = gtk4::Label::new(Some("Air Quality Unknown"));
    quality_label.add_css_class("quality-none");
    quality_box.append(&quality_label);

    let quality_pm25 = gtk4::Label::new(None);
    quality_box.append(&quality_pm25);

    let quality_warn = gtk4::Label::new(None);
    quality_warn.add_css_class("warn-label");
    quality_box.append(&quality_warn);

    vbox.append(&quality_box);

    // Connection status row
    let status_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    let connection_label = gtk4::Label::new(Some("Connection: offline"));
    connection_label.add_css_class("connection-offline");
    status_row.append(&connection_label);

    let availability_label = gtk4::Label::new(Some("Availability: offline"));
    availability_label.add_css_class("avail-offline");
    status_row.append(&availability_label);

    let last_update_label = gtk4::Label::new(Some(""));
    status_row.append(&last_update_label);

    vbox.append(&status_row);

    // Start / Stop buttons
    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let start_btn = gtk4::Button::with_label("Start MQTT");
    let stop_btn = gtk4::Button::with_label("Stop MQTT");
    stop_btn.set_sensitive(false);
    btn_row.append(&start_btn);
    btn_row.append(&stop_btn);
    vbox.append(&btn_row);

    // Button callbacks
    {
        let state_c = state.clone();
        let win_c: gtk4::Window = window.clone().upcast();
        start_btn.connect_clicked(move |_| {
            let mut app = state_c.borrow_mut();
            if !app.start_mqtt() {
                // show warning dialog if needed
                let msg = app.status.clone();
                drop(app);
                let dlg = gtk4::MessageDialog::builder()
                    .transient_for(&win_c)
                    .modal(true)
                    .message_type(gtk4::MessageType::Warning)
                    .buttons(gtk4::ButtonsType::Ok)
                    .text("Cannot start MQTT")
                    .secondary_text(&msg)
                    .build();
                dlg.connect_response(|d, _| d.close());
                dlg.present();
            }
        });
    }
    {
        let state_c = state;
        stop_btn.connect_clicked(move |_| {
            state_c.borrow_mut().stop_mqtt();
        });
    }

    (
        frame,
        start_btn,
        stop_btn,
        connection_label,
        availability_label,
        last_update_label,
        quality_box,
        quality_label,
        quality_pm25,
        quality_warn,
    )
}

fn build_metric_section(title: &str, gauges: &[GaugeSpec]) -> (gtk4::Frame, Vec<GaugeWidgets>) {
    let frame = gtk4::Frame::new(Some(title));
    let flow = gtk4::FlowBox::new();
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_margin_top(6);
    flow.set_margin_bottom(6);
    flow.set_margin_start(6);
    flow.set_margin_end(6);
    flow.set_row_spacing(6);
    flow.set_column_spacing(6);
    frame.set_child(Some(&flow));

    let mut gauge_widgets = Vec::new();
    for &(id, label, unit, ranges, max_value) in gauges {
        let (child, gw) = build_gauge_card(id, label, unit, ranges, max_value);
        flow.insert(&child, -1);
        gauge_widgets.push(gw);
    }
    (frame, gauge_widgets)
}

fn build_environment_section() -> (gtk4::Frame, Vec<GaugeWidgets>, gtk4::Label) {
    let frame = gtk4::Frame::new(Some("Environment"));
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    vbox.set_margin_top(6);
    vbox.set_margin_bottom(6);
    vbox.set_margin_start(6);
    vbox.set_margin_end(6);
    frame.set_child(Some(&vbox));

    let flow = gtk4::FlowBox::new();
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_row_spacing(6);
    flow.set_column_spacing(6);
    vbox.append(&flow);

    let mut gauge_widgets = Vec::new();
    for &(id, label, unit, ranges, max_value) in &[
        ("temperature", "Temperature", "°F", TEMP_RANGES, 104.0f64),
        ("humidity", "Humidity", "%", HUMIDITY_RANGES, 100.0f64),
    ] {
        let (child, gw) = build_gauge_card(id, label, unit, ranges, max_value);
        flow.insert(&child, -1);
        gauge_widgets.push(gw);
    }

    let last_topic_label = gtk4::Label::new(None);
    last_topic_label.add_css_class("last-topic");
    last_topic_label.set_halign(gtk4::Align::Start);
    vbox.append(&last_topic_label);

    (frame, gauge_widgets, last_topic_label)
}

// ── Gauge card ────────────────────────────────────────────────────────────────

fn build_gauge_card(
    metric_id: &'static str,
    label: &str,
    _unit: &'static str,
    ranges: &'static [(f64, f64, &'static str)],
    max_value: f64,
) -> (gtk4::FlowBoxChild, GaugeWidgets) {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    card.set_margin_top(8);
    card.set_margin_bottom(8);
    card.set_margin_start(8);
    card.set_margin_end(8);
    card.set_width_request(180);

    let name_label = gtk4::Label::new(Some(label));
    name_label.add_css_class("metric-name");
    card.append(&name_label);

    // Arc gauge drawing area (always visible; draws its own no-data state)
    let current_value: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let drawing_area = gtk4::DrawingArea::new();
    drawing_area.set_size_request(160, 140);
    {
        let cv = current_value.clone();
        drawing_area.set_draw_func(move |_, ctx, width, height| {
            draw_arc_gauge(ctx, width, height, cv.get(), ranges, max_value);
        });
    }
    card.append(&drawing_area);

    // Data box: text labels (shown when value is present)
    let data_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    let value_label = gtk4::Label::new(Some("--"));
    value_label.add_css_class("metric-value");
    data_box.append(&value_label);

    let quality_label = gtk4::Label::new(Some(""));
    quality_label.add_css_class("quality-none");
    data_box.append(&quality_label);

    card.append(&data_box);

    // No-data box (shown when value is absent)
    let no_data_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let no_data_label = gtk4::Label::new(Some("No Data"));
    no_data_label.add_css_class("quality-none");
    no_data_box.append(&no_data_label);
    no_data_box.set_visible(true);
    data_box.set_visible(false);
    card.append(&no_data_box);

    let child = gtk4::FlowBoxChild::new();
    child.set_child(Some(&card));

    let gw = GaugeWidgets {
        card,
        data_box,
        no_data_box,
        drawing_area,
        current_value,
        value_label,
        quality_label,
        metric_id,
        ranges,
    };
    (child, gw)
}

// ── Widget update ─────────────────────────────────────────────────────────────

fn update_widgets(app: &Air1App, w: &AppWidgets) {
    // Status bar
    w.status_label.set_text(&app.status);
    w.details_button.set_sensitive(!app.status.is_empty());

    // Start/stop button sensitivity
    w.start_button.set_sensitive(app.mqtt_handle.is_none());
    w.stop_button.set_sensitive(app.mqtt_handle.is_some());

    // Connection label
    if app.connected {
        w.connection_label.set_text("Connection: online");
        clear_css_classes(
            &w.connection_label,
            &["connection-online", "connection-offline"],
        );
        w.connection_label.add_css_class("connection-online");
    } else {
        w.connection_label.set_text("Connection: offline");
        clear_css_classes(
            &w.connection_label,
            &["connection-online", "connection-offline"],
        );
        w.connection_label.add_css_class("connection-offline");
    }

    // Availability
    let (avail_text, avail_class) = match (app.connected, app.metrics.last_update) {
        (false, _) => ("offline", "avail-offline"),
        (true, Some(ts)) => {
            let age = ts.elapsed().as_secs();
            if age <= 15 {
                ("fresh", "avail-fresh")
            } else if age <= 60 {
                ("stale", "avail-stale")
            } else {
                ("stalled", "avail-stalled")
            }
        }
        (true, None) => ("no data", "avail-nodata"),
    };
    w.availability_label
        .set_text(&format!("Availability: {avail_text}"));
    clear_css_classes(
        &w.availability_label,
        &[
            "avail-fresh",
            "avail-stale",
            "avail-stalled",
            "avail-nodata",
            "avail-offline",
        ],
    );
    w.availability_label.add_css_class(avail_class);

    // Last update
    if let Some(ts) = app.metrics.last_update {
        w.last_update_label
            .set_text(&format!("Last update: {}s ago", ts.elapsed().as_secs()));
    } else {
        w.last_update_label.set_text("");
    }

    // Overall quality banner
    update_quality_banner(app, w);

    // Last topic
    if let Some(topic) = &app.metrics.last_topic {
        w.last_topic_label.set_text(&format!("Last topic: {topic}"));
        w.last_topic_label.set_visible(true);
    } else {
        w.last_topic_label.set_visible(false);
    }

    // Section visibility
    for s in &w.sections {
        let enabled = app
            .cfg
            .dashboard
            .sections
            .iter()
            .find(|sec| sec.id == s.id)
            .map(|sec| sec.enabled)
            .unwrap_or(true);
        s.widget.set_visible(enabled);
    }

    // Gauge section: respect gauge-level enabled flags and update values
    for g in &w.gauges {
        let value = metric_value(app, g.metric_id);

        // Check if this gauge is enabled in config
        let enabled = app
            .cfg
            .dashboard
            .sections
            .iter()
            .any(|s| s.gauges.iter().any(|gg| gg.id == g.metric_id && gg.enabled));
        g.card.set_visible(enabled);

        update_gauge(g, value);
    }
}

fn update_quality_banner(app: &Air1App, w: &AppWidgets) {
    let banner_classes = [
        "banner-good",
        "banner-moderate",
        "banner-usg",
        "banner-unhealthy",
        "banner-vunhealthy",
        "banner-unknown",
    ];
    let quality_classes = [
        "quality-0",
        "quality-1",
        "quality-2",
        "quality-3",
        "quality-4",
        "quality-none",
    ];

    let (banner_class, quality_class, text, pm25_text) = if let Some(pm25) = app.metrics.pm25 {
        let idx = Air1App::quality_index(pm25, PM25_RANGES);
        let (bclass, qclass) = (banner_classes[idx], quality_classes[idx]);
        let labels = [
            "Excellent Air Quality",
            "Good Air Quality",
            "Moderate Air Quality",
            "Poor Air Quality",
            "Unhealthy Air Quality",
        ];
        let pm25_str = format!("  PM2.5: {pm25:.1} μg/m³");
        (bclass, qclass, labels[idx], pm25_str)
    } else {
        (
            "banner-unknown",
            "quality-none",
            "Air Quality Unknown",
            String::new(),
        )
    };

    clear_css_classes(&w.overall_quality_box, &banner_classes);
    w.overall_quality_box.add_css_class(banner_class);

    clear_css_classes(&w.overall_quality_label, &quality_classes);
    w.overall_quality_label.add_css_class(quality_class);
    w.overall_quality_label.set_text(text);

    w.overall_quality_pm25.set_text(&pm25_text);

    // Warnings
    let mut warn_parts = Vec::new();
    if let Some(co2) = app.metrics.co2
        && co2 > 2000.0
    {
        warn_parts.push(format!("⚠ High CO₂ {co2:.0}ppm"));
    }
    if let Some(tvoc) = app.metrics.tvoc
        && tvoc > 2200.0
    {
        warn_parts.push(format!("⚠ High VOC {tvoc:.0}ppb"));
    }
    w.overall_warnings.set_text(&warn_parts.join("  "));
    w.overall_warnings.set_visible(!warn_parts.is_empty());
}

fn update_gauge(g: &GaugeWidgets, value: Option<f64>) {
    let quality_classes = [
        "quality-0",
        "quality-1",
        "quality-2",
        "quality-3",
        "quality-4",
        "quality-none",
    ];

    if let Some(v) = value {
        g.data_box.set_visible(true);
        g.no_data_box.set_visible(false);

        g.current_value.set(Some(v));
        g.drawing_area.queue_draw();

        let idx = Air1App::quality_index(v, g.ranges);
        let label = Air1App::get_quality_label(v, g.ranges);
        let unit = gauge_unit(g.metric_id);

        g.value_label.set_text(&format!("{v:.1} {unit}"));
        g.quality_label.set_text(label);

        clear_css_classes(&g.quality_label, &quality_classes);
        g.quality_label.add_css_class(quality_classes[idx]);
    } else {
        g.data_box.set_visible(false);
        g.no_data_box.set_visible(true);

        g.current_value.set(None);
        g.drawing_area.queue_draw();
    }
}

// ── Arc gauge drawing ─────────────────────────────────────────────────────────

fn draw_arc_gauge(
    ctx: &cairo::Context,
    width: i32,
    height: i32,
    value: Option<f64>,
    ranges: &[(f64, f64, &'static str)],
    max_value: f64,
) {
    let w = width as f64;
    let h = height as f64;

    // Geometry: center slightly above mid-height so arc endpoints clear bottom
    let cx = w / 2.0;
    let cy = h * 0.60;
    let stroke_width = 12.0_f64;
    let radius = (w / 2.0 - stroke_width / 2.0 - 6.0).min(cy - stroke_width / 2.0 - 4.0);

    // Arc spans 270° from lower-left (0.75π) clockwise through top to lower-right (2.25π)
    let arc_start = PI * 0.75;
    let arc_end = PI * 2.25;
    let total_angle = arc_end - arc_start;

    ctx.set_line_cap(cairo::LineCap::Round);

    // ── Background segments (dimmed quality color per tier) ───────────────────
    for (min, max, _) in ranges.iter() {
        let start_ratio = (min / max_value).clamp(0.0, 1.0);
        let end_ratio = (max / max_value).clamp(0.0, 1.0);
        if (end_ratio - start_ratio).abs() < 1e-6 {
            continue;
        }
        let seg_start = arc_start + total_angle * start_ratio;
        let seg_end = arc_start + total_angle * end_ratio;

        let (r, g, b) = Air1App::get_quality_color(*min, ranges);
        ctx.set_source_rgba(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 0.3);
        ctx.set_line_width(stroke_width);
        ctx.new_sub_path();
        ctx.arc(cx, cy, radius, seg_start, seg_end);
        let _ = ctx.stroke();
    }

    // ── Value arc, needle, and center dot ─────────────────────────────────────
    if let Some(v) = value {
        let value_ratio = (v / max_value).clamp(0.0, 1.0);
        let value_angle = arc_start + total_angle * value_ratio;

        if value_ratio > 0.005 {
            let (r, g, b) = Air1App::get_quality_color(v, ranges);
            ctx.set_source_rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
            ctx.set_line_width(stroke_width);
            ctx.new_sub_path();
            ctx.arc(cx, cy, radius, arc_start, value_angle);
            let _ = ctx.stroke();
        }

        // Needle
        let needle_len = radius - stroke_width / 2.0;
        let nx = cx + needle_len * value_angle.cos();
        let ny = cy + needle_len * value_angle.sin();
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        ctx.set_line_width(2.5);
        ctx.move_to(cx, cy);
        ctx.line_to(nx, ny);
        let _ = ctx.stroke();

        // Center dot — dark fill with white ring
        ctx.arc(cx, cy, 5.0, 0.0, 2.0 * PI);
        ctx.set_source_rgb(40.0 / 255.0, 40.0 / 255.0, 40.0 / 255.0);
        let _ = ctx.fill();
        ctx.arc(cx, cy, 5.0, 0.0, 2.0 * PI);
        ctx.set_source_rgb(1.0, 1.0, 1.0);
        ctx.set_line_width(2.0);
        let _ = ctx.stroke();
    }
}

// ── Config dialog ─────────────────────────────────────────────────────────────

fn show_config_window(state: Rc<RefCell<Air1App>>, parent: &gtk4::Window) {
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Configuration")
        .default_width(500)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    win.set_child(Some(&vbox));

    let grid = gtk4::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(8);
    vbox.append(&grid);

    let cfg = state.borrow().cfg.mqtt.clone();
    let password_val = state.borrow().password.clone().unwrap_or_default();
    let keyring_unavailable = state.borrow().keyring_unavailable;

    // Helper to add a label + widget row
    let mut row = 0i32;
    let mut add_row = |label: &str, widget: &gtk4::Widget| {
        let lbl = gtk4::Label::new(Some(label));
        lbl.set_halign(gtk4::Align::End);
        grid.attach(&lbl, 0, row, 1, 1);
        grid.attach(widget, 1, row, 1, 1);
        row += 1;
    };

    let host_entry = gtk4::Entry::new();
    host_entry.set_text(&cfg.host);
    host_entry.set_hexpand(true);
    add_row("Host", &host_entry.clone().upcast());

    let port_spin = gtk4::SpinButton::with_range(1.0, 65535.0, 1.0);
    port_spin.set_value(cfg.port as f64);
    add_row("Port", &port_spin.clone().upcast());

    let tls_check = gtk4::CheckButton::with_label("Enabled");
    tls_check.set_active(cfg.tls);
    add_row("TLS", &tls_check.clone().upcast());

    let ca_entry = gtk4::Entry::new();
    ca_entry.set_text(
        &cfg.ca_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );
    ca_entry.set_placeholder_text(Some("(optional)"));
    ca_entry.set_hexpand(true);
    add_row("CA path", &ca_entry.clone().upcast());

    let client_id_entry = gtk4::Entry::new();
    client_id_entry.set_text(&cfg.client_id.clone().unwrap_or_default());
    client_id_entry.set_placeholder_text(Some("(optional)"));
    client_id_entry.set_hexpand(true);
    add_row("Client ID", &client_id_entry.clone().upcast());

    let username_entry = gtk4::Entry::new();
    username_entry.set_text(&cfg.username.clone().unwrap_or_default());
    username_entry.set_placeholder_text(Some("(optional)"));
    username_entry.set_hexpand(true);
    add_row("Username", &username_entry.clone().upcast());

    let password_entry = gtk4::PasswordEntry::new();
    password_entry.set_text(&password_val);
    password_entry.set_hexpand(true);
    add_row("Password", &password_entry.clone().upcast());

    let prefix_entry = gtk4::Entry::new();
    prefix_entry.set_text(&cfg.topic_prefix.clone().unwrap_or_default());
    prefix_entry.set_placeholder_text(Some("(e.g. homeassistant)"));
    prefix_entry.set_hexpand(true);
    add_row("Topic prefix", &prefix_entry.clone().upcast());

    let qos_spin = gtk4::SpinButton::with_range(0.0, 2.0, 1.0);
    qos_spin.set_value(cfg.qos as f64);
    add_row("QoS", &qos_spin.clone().upcast());

    let keepalive_spin = gtk4::SpinButton::with_range(5.0, 1200.0, 1.0);
    keepalive_spin.set_value(cfg.keepalive_secs as f64);
    add_row("Keepalive (s)", &keepalive_spin.clone().upcast());

    let remember_check = gtk4::CheckButton::with_label("Remember password in system keyring");
    remember_check.set_active(cfg.remember_password);
    remember_check.set_sensitive(!keyring_unavailable);
    add_row("", &remember_check.clone().upcast());

    if keyring_unavailable {
        let warn = gtk4::Label::new(Some("Keyring unavailable — session-only"));
        warn.add_css_class("warn-label");
        vbox.append(&warn);

        let help_btn = gtk4::Button::with_label("Keyring help");
        let parent_c = parent.clone();
        help_btn.connect_clicked(move |_| show_keyring_help(&parent_c));
        vbox.append(&help_btn);
    }

    // Status label
    let status_lbl = gtk4::Label::new(None);
    vbox.append(&status_lbl);
    {
        let s = state.borrow().status.clone();
        status_lbl.set_text(&s);
    }

    // Buttons
    let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    vbox.append(&btn_box);

    let save_btn = gtk4::Button::with_label("Save settings");
    let test_btn = gtk4::Button::with_label("Test connection");
    let forget_btn = gtk4::Button::with_label("Forget saved password");
    let close_btn = gtk4::Button::with_label("Close");
    btn_box.append(&save_btn);
    btn_box.append(&test_btn);
    btn_box.append(&forget_btn);
    btn_box.append(&close_btn);

    // Save
    {
        let state_c = state.clone();
        let host_e = host_entry.clone();
        let port_s = port_spin.clone();
        let tls_c = tls_check.clone();
        let ca_e = ca_entry.clone();
        let cid_e = client_id_entry.clone();
        let uname_e = username_entry.clone();
        let pw_e = password_entry.clone();
        let prefix_e = prefix_entry.clone();
        let qos_s = qos_spin.clone();
        let ka_s = keepalive_spin.clone();
        let rem_c = remember_check.clone();
        let status_l = status_lbl.clone();
        save_btn.connect_clicked(move |_| {
            let mut app = state_c.borrow_mut();
            app.cfg.mqtt.host = host_e.text().to_string();
            app.cfg.mqtt.port = port_s.value() as u16;
            app.cfg.mqtt.tls = tls_c.is_active();
            let ca = ca_e.text().to_string();
            app.cfg.mqtt.ca_path = if ca.trim().is_empty() {
                None
            } else {
                Some(ca.into())
            };
            let cid = cid_e.text().to_string();
            app.cfg.mqtt.client_id = if cid.trim().is_empty() {
                None
            } else {
                Some(cid)
            };
            let uname = uname_e.text().to_string();
            app.cfg.mqtt.username = if uname.trim().is_empty() {
                None
            } else {
                Some(uname)
            };
            let pw = pw_e.text().to_string();
            app.password = if pw.is_empty() { None } else { Some(pw) };
            let prefix = prefix_e.text().to_string();
            app.cfg.mqtt.topic_prefix = if prefix.trim().is_empty() {
                None
            } else {
                Some(prefix)
            };
            app.cfg.mqtt.qos = qos_s.value() as u8;
            app.cfg.mqtt.keepalive_secs = ka_s.value() as u16;
            app.cfg.mqtt.remember_password = rem_c.is_active();
            app.save_all();
            status_l.set_text(&app.status);
        });
    }

    // Test connection
    {
        let state_c = state.clone();
        let test_b = test_btn.clone();
        let status_l = status_lbl.clone();
        test_btn.connect_clicked(move |_| {
            test_b.set_sensitive(false);
            let mut app = state_c.borrow_mut();
            app.spawn_test_connection();
            status_l.set_text(&app.status);
        });
    }

    // Forget password
    {
        let state_c = state.clone();
        let status_l = status_lbl.clone();
        forget_btn.connect_clicked(move |_| {
            let mut app = state_c.borrow_mut();
            app.forget_password();
            status_l.set_text(&app.status);
        });
    }

    // Close
    {
        let win_c = win.clone();
        close_btn.connect_clicked(move |_| win_c.close());
    }

    win.present();
}

// ── Layout editor ─────────────────────────────────────────────────────────────

fn show_layout_window(state: Rc<RefCell<Air1App>>, parent: &gtk4::Window) {
    let win = gtk4::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Edit Dashboard Layout")
        .default_width(400)
        .default_height(500)
        .build();

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    vbox.set_margin_top(12);
    vbox.set_margin_bottom(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    win.set_child(Some(&vbox));

    let scroll = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    vbox.append(&scroll);

    let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    scroll.set_child(Some(&list_box));

    rebuild_layout_list(&list_box, state.clone());

    // Buttons
    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    vbox.append(&btn_row);

    let reset_btn = gtk4::Button::with_label("Reset layout");
    let save_btn = gtk4::Button::with_label("Save layout");
    let close_btn = gtk4::Button::with_label("Close");
    btn_row.append(&reset_btn);
    btn_row.append(&save_btn);
    btn_row.append(&close_btn);

    {
        let state_c = state.clone();
        let list_c = list_box.clone();
        reset_btn.connect_clicked(move |_| {
            state_c.borrow_mut().cfg.dashboard = config::DashboardConfig::default();
            rebuild_layout_list(&list_c, state_c.clone());
        });
    }
    {
        let state_c = state.clone();
        save_btn.connect_clicked(move |_| {
            state_c.borrow_mut().save_all();
        });
    }
    {
        let win_c = win.clone();
        close_btn.connect_clicked(move |_| win_c.close());
    }

    win.present();
}

fn rebuild_layout_list(list_box: &gtk4::Box, state: Rc<RefCell<Air1App>>) {
    // Remove all existing children
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let section_count = state.borrow().cfg.dashboard.sections.len();
    for idx in 0..section_count {
        let title = Air1App::section_title(&state.borrow().cfg.dashboard.sections[idx].id);

        // ── Section header row ─────────────────────────────────────────────
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

        let check = gtk4::CheckButton::new();
        check.set_active(state.borrow().cfg.dashboard.sections[idx].enabled);
        row.append(&check);

        let lbl = gtk4::Label::new(Some(&title));
        lbl.set_hexpand(true);
        lbl.set_halign(gtk4::Align::Start);
        row.append(&lbl);

        let up_btn = gtk4::Button::with_label("↑");
        let down_btn = gtk4::Button::with_label("↓");
        up_btn.set_sensitive(idx > 0);
        down_btn.set_sensitive(idx + 1 < section_count);
        row.append(&up_btn);
        row.append(&down_btn);

        // Toggle section enabled
        {
            let state_c = state.clone();
            check.connect_toggled(move |c| {
                state_c.borrow_mut().cfg.dashboard.sections[idx].enabled = c.is_active();
            });
        }
        // Move up
        {
            let state_c = state.clone();
            let list_c = list_box.clone();
            up_btn.connect_clicked(move |_| {
                if idx > 0 {
                    state_c
                        .borrow_mut()
                        .cfg
                        .dashboard
                        .sections
                        .swap(idx, idx - 1);
                    rebuild_layout_list(&list_c, state_c.clone());
                }
            });
        }
        // Move down
        {
            let state_c = state.clone();
            let list_c = list_box.clone();
            down_btn.connect_clicked(move |_| {
                let len = state_c.borrow().cfg.dashboard.sections.len();
                if idx + 1 < len {
                    state_c
                        .borrow_mut()
                        .cfg
                        .dashboard
                        .sections
                        .swap(idx, idx + 1);
                    rebuild_layout_list(&list_c, state_c.clone());
                }
            });
        }

        list_box.append(&row);

        // ── Per-gauge rows (indented) ──────────────────────────────────────
        let gauge_count = state.borrow().cfg.dashboard.sections[idx].gauges.len();
        for gidx in 0..gauge_count {
            let gauge_id = state.borrow().cfg.dashboard.sections[idx].gauges[gidx]
                .id
                .clone();
            let gauge_enabled = state.borrow().cfg.dashboard.sections[idx].gauges[gidx].enabled;
            let gauge_name = Air1App::gauge_label(&gauge_id);

            let gauge_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            gauge_row.set_margin_start(24); // indent under section

            let gauge_check = gtk4::CheckButton::new();
            gauge_check.set_active(gauge_enabled);
            gauge_row.append(&gauge_check);

            let gauge_lbl = gtk4::Label::new(Some(&gauge_name));
            gauge_lbl.set_halign(gtk4::Align::Start);
            gauge_row.append(&gauge_lbl);

            {
                let state_c = state.clone();
                gauge_check.connect_toggled(move |c| {
                    state_c.borrow_mut().cfg.dashboard.sections[idx].gauges[gidx].enabled =
                        c.is_active();
                });
            }

            list_box.append(&gauge_row);
        }
    }
}

// ── Keyring help ──────────────────────────────────────────────────────────────

fn show_keyring_help(parent: &gtk4::Window) {
    let msg = "The system keyring is not available. Saved passwords cannot be stored securely.\n\n\
        Common fixes:\n\
        • Ubuntu/Debian: sudo apt install gnome-keyring libsecret-1-0\n\
        • Fedora: sudo dnf install gnome-keyring libsecret\n\
        • Arch/Manjaro: sudo pacman -S gnome-keyring libsecret\n\
        • openSUSE: sudo zypper install gnome-keyring libsecret\n\n\
        Ensure the keyring daemon is running and unlocked at session start.\n\
        On headless servers, disable 'Remember password' in settings.";
    let dlg = gtk4::MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .message_type(gtk4::MessageType::Info)
        .buttons(gtk4::ButtonsType::Close)
        .text("Keyring Unavailable")
        .secondary_text(msg)
        .build();
    dlg.connect_response(|d, _| d.close());
    dlg.present();
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn metric_value(app: &Air1App, id: &str) -> Option<f64> {
    match id {
        "pm25" => app.metrics.pm25,
        "pm10" => app.metrics.pm10,
        "pm1" => app.metrics.pm1,
        "co2" => app.metrics.co2,
        "tvoc" => app.metrics.tvoc,
        "temperature" => app.metrics.temp,
        "humidity" => app.metrics.humidity,
        _ => None,
    }
}

fn gauge_unit(id: &str) -> &'static str {
    match id {
        "pm25" | "pm10" | "pm1" => "μg/m³",
        "co2" => "ppm",
        "tvoc" => "ppb",
        "temperature" => "°F",
        "humidity" => "%",
        _ => "",
    }
}

fn clear_css_classes(widget: &impl IsA<gtk4::Widget>, classes: &[&str]) {
    for c in classes {
        widget.remove_css_class(c);
    }
}
