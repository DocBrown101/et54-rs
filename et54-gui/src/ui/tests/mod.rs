use super::controls::BasicMode;
use super::*;
use crate::worker::{BatteryReadback, BatterySettings, ControlState};
use et54::{ChannelStatus, DeviceInfo, InputState, Measurements, Mode, Model, Protection, ProtectionLimits, VoltageThresholds};
use std::time::Instant;

#[test]
fn headless_ui_handles_nonfinite_timing_input_without_panicking() {
    let ctx = egui::Context::default();
    let mut dashboard = Dashboard::new(String::new());
    dashboard.connection.read_s = f64::NAN;
    dashboard.connection.write_s = f64::INFINITY;
    dashboard.monitor.poll_s = f64::NAN;
    // A unit-test fixture, never shown as hardware data in the actual application.
    let state = Snapshot {
        connected: true,
        sample: Some(crate::worker::Sample {
            values: Measurements {
                voltage: 12.,
                current: 1.,
                power: 12.,
                resistance: Some(12.),
            },
            when: Instant::now(),
            sequence: 1,
            channel: 1,
            epoch: 0,
        }),
        ..Default::default()
    };
    let mut output = ctx.run_ui(Default::default(), |ui| {
        dashboard.show(ui, &state, false, &mut None);
    });
    // Headless test has no renderer to upload the generated font atlas.
    output.textures_delta.clear();
    assert!(!output.shapes.is_empty(), "the entire UI must still render with invalid draft values");
}

#[test]
fn dashboard_adapts_to_device_control_state_and_window_width() {
    fn text_shapes(shape: &egui::Shape, result: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(text) => result.push((
                text.galley.job.text.clone(),
                text.galley
                    .rect
                    .translate(text.pos.to_vec2()),
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_shapes(shape, result);
                }
            }
            _ => {}
        }
    }
    fn render(
        ctx: &egui::Context,
        dashboard: &mut Dashboard,
        state: &Snapshot,
        size: [f32; 2],
        events: Vec<egui::Event>,
    ) -> (Vec<(String, egui::Rect)>, Vec<Command>) {
        let mut commands = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                events,
                ..Default::default()
            },
            |ui| commands = dashboard.show(ui, state, state.busy, &mut None),
        );
        output.textures_delta.clear();
        let mut texts = Vec::new();
        for clipped in output.shapes {
            text_shapes(&clipped.shape, &mut texts);
        }
        (texts, commands)
    }

    let ctx = egui::Context::default();
    configure_ui_style(&ctx);
    let mut dashboard = Dashboard::new(String::new());
    dashboard.language = Language::German;
    let mut state = Snapshot::default();
    let (texts, _) = render(&ctx, &mut dashboard, &state, [1180., 1000.], vec![]);
    assert!(
        texts
            .iter()
            .any(|(s, _)| s == "Gerät verbinden")
    );
    assert!(
        !texts
            .iter()
            .any(|(s, _)| s == "Steuerung")
    );
    assert!(
        !texts
            .iter()
            .any(|(s, _)| s == "EINGANG AUS")
    );
    assert!(
        !texts
            .iter()
            .any(|(s, _)| s == "Geräte- und Verbindungsdetails")
    );

    // UI-only fixtures: no physical connection or hardware command is made.
    state.connected = true;
    state.status = Some(ChannelStatus {
        input: InputState::Off,
        mode: Mode::Cc,
        protection: Protection::None,
    });
    for (model, control, width, theme) in [
        (Model::Et5406APlus, ControlState::Remote, 1180., egui::Theme::Dark),
        (Model::Et5420APlus, ControlState::Local, 480., egui::Theme::Light),
    ] {
        ctx.set_theme(theme);
        dashboard.controls.battery_view = false;
        state.info = Some(DeviceInfo {
            model,
            serial_number: None,
            firmware: "test".into(),
            hardware: "test".into(),
            raw: String::new(),
        });
        state.control = control;
        for (mode, label) in [
            (BasicMode::Cc, "Sollstrom"),
            (BasicMode::Cv, "Sollspannung"),
            (BasicMode::Cp, "Sollleistung"),
            (BasicMode::Cr, "Sollwiderstand"),
        ] {
            dashboard.controls.basic_mode = mode;
            let (texts, _) = render(&ctx, &mut dashboard, &state, [width, 1800.], vec![]);
            let has = |label: &str| texts.iter().any(|(s, _)| s == label);
            assert_eq!(has("Kanal 1"), model.channel_count() > 1);
            assert_eq!(has("Lokal bedienen"), control == ControlState::Remote);
            assert_eq!(has("Fernsteuerung fortsetzen"), control != ControlState::Remote);
            assert!(has(label), "missing setpoint label: {label}");
            assert!(has("Nicht angewendet"));
            assert!(!has("Gerät verbinden"));
            let details: Vec<_> = texts
                .iter()
                .filter(|(s, _)| s == "Geräte- und Verbindungsdetails")
                .collect();
            assert_eq!(details.len(), 1, "device details must only appear in the header");
            let disconnect = texts
                .iter()
                .find(|(s, _)| s == "Trennen")
                .unwrap()
                .1;
            assert!(details[0].1.left() > disconnect.right());
            assert!(
                (details[0].1.center().y - disconnect.center().y).abs() < 1.,
                "device details must share the disconnect row"
            );
            for label in ["Deutsch", "English"] {
                let language = texts
                    .iter()
                    .find(|(s, _)| s == label)
                    .unwrap()
                    .1;
                assert!(
                    language.left() >= 0. && language.right() <= width,
                    "language controls must remain inside the window: {language:?}"
                );
                assert!(
                    !language.intersects(details[0].1),
                    "device details must not overlap language controls at width {width}: {language:?} vs {:?}",
                    details[0].1
                );
            }
            if width >= 900. {
                let controls = texts
                    .iter()
                    .find(|(s, _)| s == "Steuerung")
                    .unwrap()
                    .1;
                assert!(controls.top() - disconnect.bottom() < 30., "no separate connection row may remain");
            }
        }
        dashboard.controls.battery_view = true;
        let (texts, _) = render(&ctx, &mut dashboard, &state, [width, 1800.], vec![]);
        for label in ["Entladestrom", "Batterie-Abschaltspannung", "Entladung starten", "Batterie · Gerätezähler"] {
            assert!(texts.iter().any(|(s, _)| s == label), "missing battery UI: {label}");
        }
        assert!(
            !texts
                .iter()
                .any(|(s, _)| s == "Sollwiderstand")
        );
    }

    state.busy = true;
    let (texts, _) = render(&ctx, &mut dashboard, &state, [480., 600.], vec![]);
    let off = texts
        .iter()
        .find(|(s, _)| s == "EINGANG AUS")
        .unwrap()
        .1;
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480., 600.)).contains_rect(off));
    let mut commands = Vec::new();
    for pressed in [true, false] {
        commands.extend(
            render(
                &ctx,
                &mut dashboard,
                &state,
                [480., 600.],
                vec![
                    egui::Event::PointerMoved(off.center()),
                    egui::Event::PointerButton {
                        pos: off.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .1,
        );
    }
    assert!(matches!(commands.as_slice(), [Command::Disable]), "OFF must remain usable while busy");

    // Valid battery settings must remain applicable even with a higher independent V-off.
    state.busy = false;
    state.control = ControlState::Remote;
    state.basic_setpoints = Some([1., 12., 20., 10.]);
    dashboard.controls.battery_draft = BatterySettings { current: 1., cutoff: 3. };
    dashboard.controls.protections = ProtectionLimits {
        voltage: 24.,
        current: 4.,
        power: 85.,
    };
    dashboard.controls.voltage_thresholds = VoltageThresholds { on: 5., off: 4. };
    let (texts, _) = render(&ctx, &mut dashboard, &state, [1180., 1000.], vec![]);
    assert!(
        texts
            .iter()
            .any(|(s, _)| s.contains("V-off (4.000 V)"))
    );
    let apply = texts
        .iter()
        .find(|(s, _)| s == "Anwenden (zuerst AUS)")
        .unwrap()
        .1;
    commands.clear();
    for pressed in [true, false] {
        commands.extend(
            render(
                &ctx,
                &mut dashboard,
                &state,
                [1180., 1000.],
                vec![
                    egui::Event::PointerMoved(apply.center()),
                    egui::Event::PointerButton {
                        pos: apply.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            )
            .1,
        );
    }
    assert!(
        matches!(commands.as_slice(), [Command::ConfigureBattery { .. }]),
        "the apply button must accept the click"
    );

    // Retained counters and an actual BATT profile must not override the selected view.
    state.status.as_mut().unwrap().mode = Mode::Battery;
    state.battery = Some(BatteryReadback {
        config: dashboard
            .controls
            .battery_draft
            .to_battery_config(),
        stages: 2,
    });
    state.battery_state = crate::worker::BatteryState::Stopped;
    state.battery_sample = Some(crate::worker::BatterySample {
        capacity_ah: 0.5,
        energy_wh: 1.5,
        when: Instant::now(),
        channel: dashboard.controls.selected,
        epoch: state.epoch,
    });
    for battery_view in [true, false, true] {
        dashboard.controls.battery_view = battery_view;
        let (texts, _) = render(&ctx, &mut dashboard, &state, [1180., 1800.], vec![]);
        for label in [
            "Batterie · Gerätezähler",
            "Kapazität: 0.5000 Ah",
            "Energie: 1.5000 Wh",
            "Gestoppt · Eingang AUS bestätigt",
            "Zurückgelesene Batterieeinstellungen",
        ] {
            assert_eq!(
                texts.iter().any(|(s, _)| s == label),
                battery_view,
                "battery element must follow the selected view: {label}"
            );
        }
        assert_eq!(
            texts
                .iter()
                .any(|(s, _)| s.contains("Andere Batteriekonfiguration aktiv")),
            battery_view
        );
        assert_eq!(
            state
                .battery_sample
                .unwrap()
                .capacity_ah,
            0.5
        );
        assert_eq!(state.battery_sample.unwrap().energy_wh, 1.5);
    }

    dashboard.connection.port = "draft-port".into();
    dashboard.controls.setpoints = [1.25, 12.5, 25., 50.];
    let battery_draft = dashboard.controls.battery_draft;
    let thresholds = dashboard.controls.voltage_thresholds;
    state.message = Text::OffConfirmed.into();
    for (language, button) in [(Language::English, "English"), (Language::German, "Deutsch")] {
        let (texts, _) = render(&ctx, &mut dashboard, &state, [1180., 1800.], vec![]);
        let position = texts
            .iter()
            .find(|(s, _)| s == button)
            .unwrap()
            .1
            .center();
        for pressed in [true, false] {
            let (_, commands) = render(
                &ctx,
                &mut dashboard,
                &state,
                [1180., 1800.],
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            assert!(commands.is_empty(), "changing language must not emit device commands");
        }
        assert_eq!(dashboard.language, language);
        for width in [480., 1180.] {
            ctx.set_theme(egui::Theme::Dark);
            let (texts, commands) = render(&ctx, &mut dashboard, &state, [width, 1800.], vec![]);
            assert!(commands.is_empty());
            for key in [
                Text::SwitchOff,
                Text::OffConfirmed,
                Text::Controls,
                Text::DischargeCurrent,
                Text::BatteryCounters,
            ] {
                assert!(
                    texts
                        .iter()
                        .any(|(s, _)| s == language.text(key)),
                    "missing {language:?}/{key:?}"
                );
            }
            for (icon, theme) in [("☀", egui::Theme::Light), ("🌙", egui::Theme::Dark)] {
                let (texts, _) = render(&ctx, &mut dashboard, &state, [width, 1800.], vec![]);
                let rect = |label: &str| {
                    texts
                        .iter()
                        .find(|(text, _)| text == label)
                        .unwrap()
                        .1
                };
                let toggle = rect(icon);
                let off = rect(language.text(Text::SwitchOff));
                let disconnect = rect(language.text(Text::Disconnect));
                assert!(toggle.right() > width - 30., "theme switch must stay at the right edge");
                assert!(off.right() < disconnect.left());
                for label in [language.text(Text::Disconnect), language.text(Text::DeviceDetails)] {
                    let bounds = rect(label);
                    assert!((bounds.center().y - off.center().y).abs() < 2., "{label} must share the OFF row");
                    assert!(bounds.left() >= 0. && bounds.right() <= width, "{label} must fit at width {width}");
                }
                for label in ["English", "Deutsch", icon] {
                    let bounds = rect(label);
                    assert!((bounds.center().y - toggle.center().y).abs() < 2., "{label} must share the theme-switch row");
                    assert!(bounds.left() >= 0. && bounds.right() <= width, "{label} must fit at width {width}");
                    assert!(
                        !bounds.intersects(rect(language.text(Text::DeviceDetails))),
                        "settings must not overlap device details"
                    );
                }
                if width >= 900. {
                    assert!(
                        (toggle.center().y - off.center().y).abs() < 2.,
                        "wide windows must keep all header actions on one row"
                    );
                } else {
                    assert!(toggle.top() > off.bottom(), "narrow windows must wrap settings below device actions");
                }
                for pressed in [true, false] {
                    let (_, commands) = render(
                        &ctx,
                        &mut dashboard,
                        &state,
                        [width, 1800.],
                        vec![
                            egui::Event::PointerMoved(toggle.center()),
                            egui::Event::PointerButton {
                                pos: toggle.center(),
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                    assert!(commands.is_empty(), "changing theme must not emit device commands");
                }
                assert_eq!(ctx.theme(), theme);
            }
        }
        assert_eq!(dashboard.connection.port, "draft-port");
        assert_eq!(dashboard.controls.setpoints, [1.25, 12.5, 25., 50.]);
        assert_eq!(dashboard.controls.battery_draft, battery_draft);
        assert_eq!(dashboard.controls.voltage_thresholds, thresholds);
        assert_eq!(
            state
                .battery_sample
                .unwrap()
                .capacity_ah,
            0.5
        );
    }
}
