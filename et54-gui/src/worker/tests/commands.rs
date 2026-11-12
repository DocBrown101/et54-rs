use super::*;
use et54::transport;
use et54::{LoadMode, Measurements, Model, ProtectionLimits, Range, VoltageThresholds};

#[test]
fn refresh_reads_inactive_setpoints_and_discards_incomplete_readbacks() {
    let mut updated = CONNECTED[1..].to_vec();
    for (command, response) in &mut updated {
        *response = match *command {
            "CURR1:CC?" => "R2",
            "VOLT1:CV?" => "R15",
            "POWE1:CP?" => "R175",
            "RESI1:CR?" => "R25",
            _ => *response,
        };
    }
    let mut script = CONNECTED.to_vec();
    script.extend_from_slice(&updated);
    let fail_at = updated
        .iter()
        .position(|(c, _)| *c == "RESI1:CR?")
        .unwrap();
    script.extend_from_slice(&updated[..fail_at]);
    script.push(("RESI1:CR?", "Rcmd err"));
    let (worker, observed) = mock_worker(MockTransport::pairs(&script));
    assert!(wait(&worker, 1).has_current_settings());
    worker
        .send(2, Command::Refresh)
        .unwrap();
    let refreshed = wait(&worker, 2);
    assert!(refreshed.error.is_none());
    assert_eq!(refreshed.settings, Some(LoadMode::ConstantCurrent(2.)));
    assert_eq!(refreshed.basic_setpoints, Some([2., 15., 175., 25.]));
    worker
        .send(3, Command::Refresh)
        .unwrap();
    let failed = wait(&worker, 3);
    assert!(failed.error.is_some());
    assert!(failed.basic_setpoints.is_none() && !failed.has_current_settings());
    // A failed read must not leave an old or partially updated configuration usable.
    worker.send(4, Command::Enable).unwrap();
    assert!(
        wait(&worker, 4)
            .error
            .unwrap()
            .render(Language::English)
            .contains("read channel settings")
    );
    drop(worker);
    observed
        .lock()
        .unwrap()
        .assert_finished();
}

#[test]
fn mode_change_invalidates_settings_until_a_complete_refresh() {
    let mut script = CONNECTED.to_vec();
    for _ in 0..2 {
        script.extend([("MEAS1:ALL?", "R1 12 12 12"), ("Ch1:SW?", "OFF"), ("Ch1:MODE?", "CV"), ("LOAD1:ABNO?", "NONE")]);
    }
    script.extend([
        ("Ch1:SW?", "OFF"),
        ("Ch1:MODE?", "CV"),
        ("LOAD1:ABNO?", "NONE"),
        ("Ch1:MODE?", "CV"),
        ("VOLT1:CV?", "R12"),
        ("CURR1:CC?", "R1"),
        ("VOLT1:CV?", "R12"),
        ("POWE1:CP?", "R20"),
        ("RESI1:CR?", "R10"),
        ("VOLT1:VMAX?", "R24"),
        ("CURR1:IMAX?", "R4"),
        ("POWE1:PMAX?", "R85"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("LOAD1:CRANGE?", "HIGH"),
        ("VOLT1:ON?", "R2"),
        ("VOLT1:OFF?", "R1.5"),
    ]);
    let mut device = Et54::connect(
        MockTransport::pairs(&script),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .unwrap();
    let mut state = Snapshot::default();
    refresh_channel_settings(&mut device, &mut state).unwrap();
    assert!(state.has_current_settings());
    let revision = state.settings_revision;
    let mut sequence = 0;
    for _ in 0..2 {
        poll_channel_measurements(&mut device, &mut state, &mut sequence).unwrap();
        assert_eq!(state.status.unwrap().mode, Mode::Cv);
        assert!(state.sample.is_some());
        assert!(state.settings.is_none());
        assert!(state.basic_setpoints.is_none());
        assert!(state.protections.is_none() && state.ranges.is_none());
        assert!(state.voltage_thresholds.is_none());
        assert!(!state.has_current_settings());
        assert_eq!(state.settings_revision, revision + 1);
    }
    refresh_channel_settings(&mut device, &mut state).unwrap();
    assert_eq!(state.settings, Some(LoadMode::ConstantVoltage(12.)));
    assert_eq!(state.voltage_thresholds, Some(VoltageThresholds { on: 2., off: 1.5 }));
    assert!(state.has_current_settings());
    assert_eq!(state.settings_revision, revision + 2);
    device.transport().assert_finished();
}

#[test]
fn local_errors_preserve_readbacks_and_device_errors_retract_success() {
    let mut script = CONNECTED.to_vec();
    script.extend([("Ch1:SW OFF", "Rexecu success"), ("Ch1:SW?", "OFF")]);
    script.extend_from_slice(&CONNECTED[1..]);
    script.push(("Ch1:SW?", "Rcmd err"));
    script.extend_from_slice(&CONNECTED[1..]);
    script.push(("Ch1:SW OFF", "Rexecu err"));
    let (worker, _) = mock_worker(MockTransport::pairs(&script));
    assert!(wait(&worker, 1).connected);
    worker
        .send(2, Command::Interval(Duration::from_secs(60)))
        .unwrap();
    let before = wait(&worker, 2);
    worker
        .send(3, Command::Interval(Duration::ZERO))
        .unwrap();
    let after = wait(&worker, 3);
    assert!(
        after
            .error
            .as_ref()
            .unwrap()
            .render(Language::English)
            .contains("poll pause")
    );
    assert!(after.connected);
    assert_eq!(after.status, before.status);
    assert_eq!(after.settings, before.settings);
    assert_eq!(after.protections, before.protections);
    assert_eq!(after.ranges, before.ranges);
    assert_eq!(after.settings_revision, before.settings_revision);
    assert_eq!(after.message, before.message);

    worker
        .send(
            4,
            Command::Configure {
                mode: LoadMode::ConstantCurrent(f64::NAN),
                protections: before.protections.unwrap(),
                ranges: before.ranges.unwrap(),
                voltage_thresholds: before.voltage_thresholds.unwrap(),
            },
        )
        .unwrap();
    let rejected = wait(&worker, 4);
    assert!(
        rejected
            .error
            .as_ref()
            .unwrap()
            .render(Language::English)
            .contains("invalid parameter")
    );
    assert_eq!(rejected.status, before.status);
    assert_eq!(rejected.settings, before.settings);

    worker
        .send(5, Command::Disable)
        .unwrap();
    let off = wait(&worker, 5);
    assert!(off.error.is_none());
    assert_eq!(off.message, Text::OffConfirmed.into());
    worker
        .send(6, Command::Refresh)
        .unwrap();
    let failed = wait(&worker, 6);
    assert!(
        failed
            .error
            .as_ref()
            .unwrap()
            .render(Language::English)
            .contains("Rcmd err")
    );
    assert!(failed.connected);
    assert!(failed.status.is_none() && failed.sample.is_none());
    assert!(failed.settings.is_none());
    assert!(failed.protections.is_none() && failed.ranges.is_none());
    assert_eq!(failed.message, Text::CommandFailedUnknown.into());

    // Even a queued ON must not proceed without a complete readback.
    worker.send(7, Command::Enable).unwrap();
    let blocked = wait(&worker, 7);
    assert!(
        blocked
            .error
            .unwrap()
            .render(Language::English)
            .contains("read channel settings")
    );
    worker
        .send(8, Command::Refresh)
        .unwrap();
    let recovered = wait(&worker, 8);
    assert!(recovered.error.is_none());
    assert!(recovered.has_current_settings());
    assert_eq!(recovered.message, Text::SettingsRead.into());
    worker
        .send(9, Command::Disable)
        .unwrap();
    let failed_off = wait(&worker, 9);
    assert!(!failed_off.connected);
    assert!(
        failed_off
            .error
            .unwrap()
            .render(Language::English)
            .contains("SWITCH-OFF FAILED")
    );
    assert_eq!(failed_off.message, Text::CommunicationFailed.into());
}

#[test]
fn configure_validates_thresholds_before_off_and_requires_complete_readback() {
    for failure in [None, Some("write"), Some("readback")] {
        let mut script = CONNECTED.to_vec();
        script.extend([
            ("Ch1:SW OFF", "Rexecu success"),
            ("Ch1:SW?", "OFF"),
            ("LOAD1:VRANGE HIGH", "Rexecu success"),
            ("LOAD1:CRANGE HIGH", "Rexecu success"),
            ("LOAD1:VRANGE?", "HIGH"),
            ("VOLT1:ON 12", "Rexecu success"),
            ("VOLT1:OFF 10", if failure == Some("write") { "Rexecu err" } else { "Rexecu success" }),
        ]);
        if failure != Some("write") {
            script.extend([
                ("VOLT1:VMAX 125", "Rexecu success"),
                ("CURR1:IMAX 21", "Rexecu success"),
                ("POWE1:PMAX 85", "Rexecu success"),
                ("CURR1:CC 1", "Rexecu success"),
                ("Ch1:MODE CC", "Rexecu success"),
            ]);
            script.extend_from_slice(&CONNECTED[1..CONNECTED.len() - 2]);
            script.extend([("VOLT1:ON?", "R12"), ("VOLT1:OFF?", if failure.is_some() { "Rcmd err" } else { "R10" })]);
        }
        let (worker, _) = mock_worker(MockTransport::pairs(&script));
        let before = wait(&worker, 1);
        assert!(before.connected);
        worker
            .send(
                2,
                Command::Configure {
                    mode: before.settings.unwrap(),
                    protections: before.protections.unwrap(),
                    ranges: before.ranges.unwrap(),
                    voltage_thresholds: VoltageThresholds { on: 12., off: f64::NAN },
                },
            )
            .unwrap();
        let rejected = wait(&worker, 2);
        assert!(
            rejected
                .error
                .unwrap()
                .render(Language::English)
                .contains("V-off")
        );
        assert_eq!(rejected.status, before.status);
        assert_eq!(rejected.voltage_thresholds, before.voltage_thresholds);
        assert_eq!(rejected.control, ControlState::Remote);
        worker
            .send(
                3,
                Command::Configure {
                    mode: before.settings.unwrap(),
                    protections: before.protections.unwrap(),
                    ranges: before.ranges.unwrap(),
                    voltage_thresholds: VoltageThresholds { on: 12., off: 10. },
                },
            )
            .unwrap();
        let applied = wait(&worker, 3);
        if failure.is_none() {
            assert!(applied.error.is_none() && applied.has_current_settings());
            assert_eq!(applied.status.unwrap().input, InputState::Off);
            assert_eq!(applied.voltage_thresholds, Some(VoltageThresholds { on: 12., off: 10. }));
        } else {
            assert!(applied.error.is_some() && !applied.has_current_settings());
            assert!(applied.status.is_none() && applied.voltage_thresholds.is_none());
            assert!(applied.basic_setpoints.is_none());
            assert_eq!(applied.control, ControlState::Unknown);
        }
    }
}

#[test]
fn local_control_pauses_polling_and_resume_reads_manual_changes() {
    let mut resumed = CONNECTED[1..].to_vec();
    let len = resumed.len();
    resumed[len - 2].1 = "R8";
    resumed[len - 1].1 = "R6";
    let mut script = CONNECTED.to_vec();
    script.push(("SYST:LOCA", "Rexecu success"));
    script.extend_from_slice(&resumed);
    script.extend([
        ("MEAS1:ALL?", "R1 12 12 12"),
        ("Ch1:SW?", "OFF"),
        ("Ch1:MODE?", "CC"),
        ("LOAD1:ABNO?", "NONE"),
        ("SYST:LOCA", "Rexecu success"),
        ("Ch1:SW OFF", "Rexecu success"),
        ("Ch1:SW?", "OFF"),
    ]);
    script.extend_from_slice(&resumed);
    let (worker, _) = mock_worker(MockTransport::pairs(&script));
    assert_eq!(wait(&worker, 1).control, ControlState::Remote);
    worker
        .send(2, Command::Interval(Duration::from_millis(500)))
        .unwrap();
    wait(&worker, 2);
    worker.send(3, Command::Local).unwrap();
    let local = wait(&worker, 3);
    assert_eq!(local.control, ControlState::Local);
    assert!(local.status.is_none() && !local.has_current_settings());
    assert!(local.voltage_thresholds.is_none());
    // Longer than the poll interval: any unintended query consumes the resume script.
    thread::sleep(Duration::from_millis(650));
    let paused = worker.state.lock().unwrap().clone();
    assert!(paused.connected && paused.error.is_none());
    assert_eq!(paused.control, ControlState::Local);
    for (id, command) in [(4, Command::Enable), (5, Command::Refresh)] {
        worker.send(id, command).unwrap();
        let blocked = wait(&worker, id);
        assert!(
            blocked
                .error
                .unwrap()
                .render(Language::English)
                .contains("resume remote control")
        );
        assert_eq!(blocked.control, ControlState::Local);
    }
    worker.send(6, Command::Remote).unwrap();
    let remote = wait(&worker, 6);
    assert!(remote.error.is_none() && remote.has_current_settings());
    assert_eq!(remote.control, ControlState::Remote);
    assert_eq!(remote.voltage_thresholds, Some(VoltageThresholds { on: 8., off: 6. }));
    let deadline = Instant::now() + Duration::from_secs(2);
    while worker
        .state
        .lock()
        .unwrap()
        .sample
        .is_none()
    {
        assert!(Instant::now() < deadline, "polling did not resume");
        thread::sleep(Duration::from_millis(5));
    }
    worker.send(7, Command::Local).unwrap();
    assert_eq!(wait(&worker, 7).control, ControlState::Local);
    worker
        .send(8, Command::Disable)
        .unwrap();
    let off = wait(&worker, 8);
    assert!(off.error.is_none() && off.has_current_settings());
    assert_eq!(off.control, ControlState::Remote);
    assert_eq!(off.status.unwrap().input, InputState::Off);
    assert!(off.sample.is_none());
}

#[test]
fn failed_unlock_or_resume_keeps_control_unknown_and_polling_paused() {
    for unlock_fails in [true, false] {
        let mut script = CONNECTED.to_vec();
        script.push(("SYST:LOCA", if unlock_fails { "Rexecu err" } else { "Rexecu success" }));
        if !unlock_fails {
            script.push(("Ch1:SW?", "Rcmd err"));
        }
        let (worker, _) = mock_worker(MockTransport::pairs(&script));
        assert!(wait(&worker, 1).connected);
        worker
            .send(2, Command::Interval(Duration::from_millis(500)))
            .unwrap();
        wait(&worker, 2);
        worker.send(3, Command::Local).unwrap();
        let mut failed = wait(&worker, 3);
        if !unlock_fails {
            assert_eq!(failed.control, ControlState::Local);
            worker.send(4, Command::Remote).unwrap();
            failed = wait(&worker, 4);
        }
        assert!(failed.error.is_some());
        assert_eq!(failed.control, ControlState::Unknown);
        assert!(failed.status.is_none() && !failed.has_current_settings());
        thread::sleep(Duration::from_millis(650));
        let paused = worker.state.lock().unwrap();
        assert!(paused.connected);
        assert_eq!(paused.control, ControlState::Unknown);
        assert_eq!(paused.error, failed.error);
    }
}

#[test]
fn backpressure_is_bounded_and_shutdown_drops_pending_commands() {
    let worker = Worker::spawn::<MockTransport, _>(egui::Context::default(), |_, _, _| {
        thread::sleep(Duration::from_millis(80));
        Err(Error::Unavailable)
    })
    .unwrap();
    worker
        .send(1, Command::Connect("mock".into(), ConnectionConfig::default()))
        .unwrap();
    thread::sleep(Duration::from_millis(10));
    let rejected = (2..100)
        .filter(|id| {
            worker
                .send(*id, Command::Enable)
                .is_err()
        })
        .count();
    assert!(rejected >= 80);
    let state = worker.state.clone();
    drop(worker);
    assert!(state.lock().unwrap().completed <= 1, "queued activations must not execute after shutdown");
}

#[test]
fn gui_validation_matches_units_and_protections() {
    let p = ProtectionLimits {
        voltage: 24.,
        current: 4.,
        power: 85.,
    };
    for (mode, ranges, valid) in [
        (LoadMode::ConstantCurrent(2.), [Range::High; 2], true),
        (LoadMode::ConstantCurrent(5.), [Range::High; 2], false),
        (LoadMode::ConstantCurrent(f64::NAN), [Range::High; 2], false),
        (LoadMode::ConstantVoltage(12.), [Range::Low; 2], false),
    ] {
        assert_eq!(
            validate_gui_settings(Model::Et5406APlus, mode, p, ranges, VoltageThresholds { on: 1.5, off: 1. }).is_ok(),
            valid,
            "{mode:?}, {ranges:?}"
        );
    }
}

#[test]
fn polling_emits_real_library_values_and_invalidates_on_disconnect() {
    let mut steps = Vec::new();
    for (command, response) in
        CONNECTED
            .iter()
            .copied()
            .chain([("MEAS1:ALL?", "R1.5 12 18 8"), ("Ch1:SW?", "OFF"), ("Ch1:MODE?", "CC"), ("LOAD1:ABNO?", "NONE")])
    {
        steps.push(transport::MockStep::Write(format!("{command}\n").into_bytes()));
        steps.push(transport::MockStep::Read(format!("{response}\r\n").into_bytes()));
    }
    steps.push(transport::MockStep::Write(b"MEAS1:ALL?\n".to_vec()));
    steps.push(transport::MockStep::ReadError(std::io::ErrorKind::BrokenPipe));
    let (worker, _) = mock_worker(MockTransport::new(steps));
    assert!(wait(&worker, 1).connected);
    worker
        .send(2, Command::Interval(Duration::from_millis(500)))
        .unwrap();
    wait(&worker, 2);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut observed = false;
    loop {
        let state = worker.state.lock().unwrap().clone();
        if let Some(sample) = state.sample {
            assert_eq!(
                sample.values,
                Measurements {
                    voltage: 12.,
                    current: 1.5,
                    power: 18.,
                    resistance: Some(8.)
                }
            );
            assert_eq!(sample.channel, 1);
            observed = true;
        }
        if observed && !state.connected {
            assert!(state.sample.is_none() && state.status.is_none());
            assert!(
                state
                    .error
                    .unwrap()
                    .render(Language::English)
                    .contains("input state unknown")
            );
            break;
        }
        assert!(Instant::now() < deadline, "poll/error event missing");
        thread::sleep(Duration::from_millis(5));
    }
}
