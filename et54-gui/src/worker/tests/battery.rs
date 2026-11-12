use super::*;
use et54::{Model, ProtectionLimits, Range, VoltageThresholds};

const BATTERY_CONFIG_READ: &[(&str, &str)] = &[
    ("BATT1:MODE?", "CC"),
    ("BATT1:BCUT?", "V"),
    ("BATT1:BCUT?", "V"),
    ("CURR1:BCC1?", "R1"),
    ("CURR1:BCC2?", "R1"),
    ("CURR1:BCC3?", "R1"),
    ("BATT1:BCUT?", "V"),
    ("BATT1:MODE?", "CC"),
    ("VOLT1:BCC1?", "R3"),
    ("VOLT1:BCC2?", "R3"),
    ("VOLT1:BCC3?", "R3"),
];

fn battery_readback(input: &'static str, stages: &'static str, capacity: &'static str, energy: &'static str) -> Vec<(&'static str, &'static str)> {
    let mut script = vec![("Ch1:SW?", input), ("Ch1:MODE?", "BATT"), ("LOAD1:ABNO?", "NONE")];
    script.extend_from_slice(&CONNECTED[6..]);
    script.extend_from_slice(BATTERY_CONFIG_READ);
    script.extend([
        ("BATT1:BAEN?", stages),
        ("BATT1:CAPA?", capacity),
        ("BATT1:ENER?", energy),
        ("Ch1:MODE?", "BATT"),
    ]);
    script
}

#[test]
fn battery_configuration_start_stop_and_failed_readback_preserve_real_counters() {
    let mut script = CONNECTED.to_vec();
    script.extend([
        ("Ch1:SW OFF", "Rexecu success"),
        ("Ch1:SW?", "OFF"),
        ("LOAD1:VRANGE HIGH", "Rexecu success"),
        ("LOAD1:CRANGE HIGH", "Rexecu success"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("VOLT1:ON 1.5", "Rexecu success"),
        ("VOLT1:OFF 1", "Rexecu success"),
        ("VOLT1:VMAX 125", "Rexecu success"),
        ("CURR1:IMAX 21", "Rexecu success"),
        ("POWE1:PMAX 85", "Rexecu success"),
        ("BATT1:MODE CC", "Rexecu success"),
        ("BATT1:BCUT V", "Rexecu success"),
        ("BATT1:BCUT?", "V"),
        ("CURR1:BCC1 1", "Rexecu success"),
        ("CURR1:BCC2 1", "Rexecu success"),
        ("CURR1:BCC3 1", "Rexecu success"),
        ("BATT1:BCUT?", "V"),
        ("BATT1:MODE?", "CC"),
        ("VOLT1:BCC1 3", "Rexecu success"),
        ("VOLT1:BCC2 3", "Rexecu success"),
        ("VOLT1:BCC3 3", "Rexecu success"),
        ("Ch1:MODE BATT", "Rexecu success"),
        ("BATT1:BAEN 1", "Rexecu success"),
    ]);
    script.extend(battery_readback("OFF", "R1", "R0.25", "R0.75"));
    // The explicit start re-reads the profile before the library's own activation preflight.
    script.extend(battery_readback("OFF", "R1", "R0.25", "R0.75"));
    script.extend([
        ("Ch1:MODE?", "BATT"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("LOAD1:CRANGE?", "HIGH"),
        ("VOLT1:VMAX?", "R125"),
        ("CURR1:IMAX?", "R21"),
        ("POWE1:PMAX?", "R85"),
        ("VOLT1:ON?", "R1.5"),
        ("VOLT1:OFF?", "R1"),
        ("LOAD1:ABNO?", "NONE"),
    ]);
    script.extend_from_slice(BATTERY_CONFIG_READ);
    script.extend([("Ch1:MODE?", "BATT"), ("Ch1:SW ON", "Rexecu success"), ("Ch1:SW?", "ON")]);
    script.extend(battery_readback("ON", "R1", "R0.25", "R0.75"));
    script.extend([("Ch1:SW OFF", "Rexecu success"), ("Ch1:SW?", "OFF")]);
    script.extend(battery_readback("OFF", "R1", "R0.5", "R1.5"));
    let mut failed = battery_readback("OFF", "R1", "R99", "Rcmd err");
    failed.pop(); // No mode query after the failed energy read.
    script.extend(failed);
    // Regression: the independent V-off threshold may exceed the battery cutoff (3 V).
    for (command, response) in &mut script {
        if *command == "VOLT1:OFF?" {
            *response = "R4";
        } else if *command == "VOLT1:OFF 1" {
            *command = "VOLT1:OFF 4";
        }
    }
    let (worker, observed) = mock_worker(MockTransport::pairs(&script));
    let connected = wait(&worker, 1);
    let configure = |battery| Command::ConfigureBattery {
        battery,
        protections: connected.protections.unwrap(),
        ranges: connected.ranges.unwrap(),
        voltage_thresholds: connected.voltage_thresholds.unwrap(),
    };
    // Rejected before even OFF is sent. The next expected wire operation is the valid apply.
    worker
        .send(2, configure(BatterySettings { current: f64::NAN, cutoff: 3. }))
        .unwrap();
    assert!(wait(&worker, 2).error.is_some());
    worker
        .send(3, configure(BatterySettings { current: 1., cutoff: 3. }))
        .unwrap();
    let configured = wait(&worker, 3);
    assert!(configured.error.is_none(), "{:?}", configured.error);
    assert!(configured.has_current_settings());
    assert_eq!(configured.battery_state, BatteryState::Ready);
    assert_eq!(configured.status.unwrap().input, InputState::Off);
    assert_eq!(configured.battery.unwrap().stages, 1);
    assert_eq!(
        configured
            .battery_sample
            .unwrap()
            .capacity_ah,
        0.25
    );
    worker.send(4, Command::Enable).unwrap();
    let started = wait(&worker, 4);
    assert!(started.error.is_none(), "{:?}", started.error);
    assert_eq!(started.battery_state, BatteryState::Active);
    worker
        .send(5, Command::Disable)
        .unwrap();
    let stopped = wait(&worker, 5);
    assert!(stopped.error.is_none(), "{:?}", stopped.error);
    assert_eq!(stopped.battery_state, BatteryState::Stopped);
    assert_eq!(
        stopped
            .battery_sample
            .unwrap()
            .capacity_ah,
        0.5
    );
    assert_eq!(
        stopped
            .battery_sample
            .unwrap()
            .energy_wh,
        1.5
    );
    worker
        .send(6, Command::Refresh)
        .unwrap();
    let failed = wait(&worker, 6);
    assert!(failed.error.is_some());
    assert!(!failed.has_current_settings());
    assert_eq!(failed.battery_state, BatteryState::Unknown);
    assert_eq!(
        failed
            .battery_sample
            .unwrap()
            .capacity_ah,
        0.5,
        "never publish partial counters"
    );
    worker.send(7, Command::Enable).unwrap();
    assert!(wait(&worker, 7).error.is_some());
    worker
        .send(8, Command::Disconnect)
        .unwrap();
    let disconnected = wait(&worker, 8);
    let retained = disconnected.battery_sample.unwrap();
    assert_eq!(retained.energy_wh, 1.5);
    assert_ne!(retained.epoch, disconnected.epoch, "retained values must be stale");
    drop(worker);
    observed
        .lock()
        .unwrap()
        .assert_finished();
}

#[test]
fn activation_rejects_multistage_or_changed_profiles_without_on() {
    for initial_stages in [Some("R1"), Some("R2"), None] {
        let mut script = if let Some(stages) = initial_stages {
            let mut script = vec![("*IDN?", "ET5406A+ SN V1 H1")];
            script.extend(battery_readback("OFF", stages, "R0", "R0"));
            script
        } else {
            CONNECTED.to_vec()
        };
        if initial_stages == Some("R1") {
            // BAEN changed since the last readback: reject before any ON write.
            script.extend(battery_readback("OFF", "R3", "R0", "R0"));
        } else if initial_stages.is_none() {
            // A basic-mode start must not accidentally activate a battery profile either.
            script.extend(battery_readback("OFF", "R1", "R0", "R0"));
        }
        let (worker, observed) = mock_worker(MockTransport::pairs(&script));
        let connected = wait(&worker, 1);
        assert!(connected.error.is_none(), "{:?}", connected.error);
        assert_eq!(connected.has_current_settings(), initial_stages != Some("R2"));
        worker.send(2, Command::Enable).unwrap();
        let blocked = wait(&worker, 2);
        assert!(blocked.error.is_some());
        drop(worker);
        observed
            .lock()
            .unwrap()
            .assert_finished();
    }
}

#[test]
fn battery_poll_observes_off_not_zero_current_and_retains_final_values_on_failure() {
    let mut script = vec![("*IDN?", "ET5406A+ SN V1 H1")];
    for input in ["ON", "OFF"] {
        script.extend([
            ("MEAS1:ALL?", "R0 3 0 OL"),
            ("Ch1:SW?", input),
            ("Ch1:MODE?", "BATT"),
            ("LOAD1:ABNO?", "NONE"),
            ("BATT1:CAPA?", "R0.5"),
            ("BATT1:ENER?", "R1.5"),
        ]);
    }
    script.push(("MEAS1:ALL?", "Rcmd err"));
    let mut d = Et54::connect(
        MockTransport::pairs(&script),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .unwrap();
    let mut state = Snapshot::default();
    let mut sequence = 0;
    poll_channel_measurements(&mut d, &mut state, &mut sequence).unwrap();
    assert_eq!(state.battery_state, BatteryState::Active, "zero current is not a stop signal");
    poll_channel_measurements(&mut d, &mut state, &mut sequence).unwrap();
    assert_eq!(state.battery_state, BatteryState::Ended);
    assert_eq!(state.message, BatteryState::Ended.label().into());
    assert_eq!(state.battery_sample.unwrap().energy_wh, 1.5);
    assert!(poll_channel_measurements(&mut d, &mut state, &mut sequence).is_err());
    clear_device(&mut state);
    assert_eq!(state.battery_state, BatteryState::Unknown);
    assert_eq!(state.battery_sample.unwrap().energy_wh, 1.5);
    d.transport().assert_finished();
}

#[test]
fn battery_validation_checks_bounds_without_coupling_independent_shutdown_thresholds() {
    let p = ProtectionLimits {
        voltage: 24.,
        current: 4.,
        power: 85.,
    };
    let thresholds = VoltageThresholds { on: 1.5, off: 1. };
    for (current, cutoff, ranges, valid) in [
        (1., 3., [Range::High; 2], true),
        (0., 3., [Range::High; 2], false),
        (f64::NAN, 3., [Range::High; 2], false),
        (1., f64::INFINITY, [Range::High; 2], false),
        (1., 0.5, [Range::High; 2], true),
        (1., 0.05, [Range::High; 2], false),
        (-1., 3., [Range::High; 2], false),
        (1., 25., [Range::High; 2], false),
        (5., 3., [Range::High; 2], false),
        (3.1, 3., [Range::High, Range::Low], false),
        (1., 21., [Range::Low, Range::High], false),
    ] {
        assert_eq!(
            BatterySettings { current, cutoff }
                .validate(Model::Et5406APlus, p, ranges, thresholds)
                .is_ok(),
            valid
        );
    }
    assert!(
        BatterySettings { current: 0., cutoff: 3. }
            .validate(Model::Et5406APlus, p, [Range::High; 2], thresholds)
            .unwrap_err()
            .to_string()
            .contains(Text::PositiveCurrent.english())
    );
}
