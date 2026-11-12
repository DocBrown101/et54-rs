use super::*;
use et54::{ChannelStatus, Protection};

#[test]
fn mode_drafts_and_device_readbacks_keep_setpoints_and_channels_separate() {
    let mut controls = Controls::default();
    // Readback fixtures only: no physical device or hardware writes.
    let mut state = Snapshot {
        epoch: 1,
        settings_revision: 1,
        settings: Some(LoadMode::ConstantPower(175.)),
        basic_setpoints: Some([2., 12., 175., 10.]),
        protections: Some(controls.protections),
        ranges: Some(controls.ranges),
        voltage_thresholds: Some(controls.voltage_thresholds),
        ..Default::default()
    };
    controls.sync_from_snapshot(&state);
    assert_eq!(controls.basic_mode, BasicMode::Cp, "initial selection follows the device");
    assert!(!controls.settings_changed(&state));
    for (mode, expected) in [
        (BasicMode::Cc, LoadMode::ConstantCurrent(2.)),
        (BasicMode::Cv, LoadMode::ConstantVoltage(12.)),
        (BasicMode::Cp, LoadMode::ConstantPower(175.)),
        (BasicMode::Cr, LoadMode::ConstantResistance(10.)),
    ] {
        controls.basic_mode = mode;
        assert_eq!(controls.draft_load_mode(), expected, "175 W must never become 175 A or V");
        assert_eq!(controls.settings_changed(&state), mode != BasicMode::Cp);
        controls.setpoints[mode.setpoint_index()] += 0.5;
    }
    controls.sync_from_snapshot(&state); // Polls without a new settings revision preserve all drafts.
    assert_eq!(controls.setpoints, [2.5, 12.5, 175.5, 10.5]);
    controls.basic_mode = BasicMode::Cv;
    state.settings_revision += 1;
    state.basic_setpoints = Some([3., 15., 175., 20.]);
    controls.sync_from_snapshot(&state);
    assert_eq!(controls.draft_load_mode(), LoadMode::ConstantVoltage(15.));
    assert!(controls.settings_changed(&state), "CV is selected but CP is still active");

    // A failed read invalidates the values, and a retry keeps the editor selection.
    state.settings_revision += 1;
    state.basic_setpoints = None;
    state.settings = None;
    controls.sync_from_snapshot(&state);
    assert_eq!(controls.setpoints, [0.; 4]);
    assert!(controls.settings_changed(&state));
    state.settings_revision += 1;
    state.basic_setpoints = Some([3., 15., 175., 20.]);
    state.settings = Some(LoadMode::ConstantPower(175.));
    controls.sync_from_snapshot(&state);
    assert_eq!(controls.draft_load_mode(), LoadMode::ConstantVoltage(15.));

    // Channel switches and reconnects replace drafts and select the device mode.
    for (epoch, channel, values, mode) in [
        (1, 2, [1., 24., 30., 50.], LoadMode::ConstantCurrent(1.)),
        (1, 1, [4., 18., 100., 25.], LoadMode::ConstantPower(100.)),
        (2, 1, [2., 10., 40., 30.], LoadMode::ConstantResistance(30.)),
    ] {
        state.epoch = epoch;
        state.selected = channel;
        state.settings_revision += 1;
        state.basic_setpoints = Some(values);
        state.settings = Some(mode);
        controls.selected = channel;
        controls.sync_from_snapshot(&state);
        assert_eq!(controls.setpoints, values);
        assert_eq!(controls.draft_load_mode(), mode);
        assert!(!controls.settings_changed(&state));
    }

    let battery = BatterySettings { current: 1., cutoff: 3. };
    state.epoch += 1;
    state.settings_revision += 1;
    state.status = Some(ChannelStatus {
        input: InputState::Off,
        mode: Mode::Battery,
        protection: Protection::None,
    });
    state.settings = None;
    state.battery = Some(BatteryReadback {
        config: battery.to_battery_config(),
        stages: 1,
    });
    controls.sync_from_snapshot(&state);
    assert!(controls.battery_view, "initial battery selection follows the actual device");
    assert_eq!(controls.battery_draft, battery);
    assert!(!controls.settings_changed(&state));
    controls.battery_view = false;
    assert!(controls.settings_changed(&state), "view selection alone never changes hardware mode");
    controls.battery_view = true;
    controls.battery_draft.current = 2.;
    controls.sync_from_snapshot(&state);
    assert_eq!(controls.battery_draft.current, 2., "polling preserves edits");
    assert!(controls.settings_changed(&state));
    state.settings_revision += 1;
    state.battery.as_mut().unwrap().stages = 3;
    controls.sync_from_snapshot(&state);
    assert!(controls.settings_changed(&state), "a multistage profile must never look applied");
    assert!(!state.has_current_settings());
    assert_eq!(state.battery.unwrap().stages, 3);
}

#[test]
fn draft_validation_and_commands_keep_basic_and_battery_settings_separate() {
    let mut controls = Controls {
        setpoints: [2., 12., 20., 10.],
        battery_draft: BatterySettings { current: 1., cutoff: 3. },
        protections: ProtectionLimits {
            voltage: 24.,
            current: 4.,
            power: 85.,
        },
        ..Default::default()
    };
    for (mode, expected) in [
        (BasicMode::Cc, LoadMode::ConstantCurrent(2.)),
        (BasicMode::Cv, LoadMode::ConstantVoltage(12.)),
        (BasicMode::Cp, LoadMode::ConstantPower(20.)),
        (BasicMode::Cr, LoadMode::ConstantResistance(10.)),
    ] {
        controls.basic_mode = mode;
        controls
            .validate_draft(Model::Et5406APlus)
            .unwrap();
        let Command::Configure {
            mode,
            protections,
            ranges,
            voltage_thresholds,
        } = controls.configure_command()
        else {
            panic!("basic view must configure a basic load mode");
        };
        assert_eq!(mode, expected);
        assert_eq!(protections, controls.protections);
        assert_eq!(ranges, controls.ranges);
        assert_eq!(voltage_thresholds, controls.voltage_thresholds);
    }
    controls.battery_view = true;
    controls
        .validate_draft(Model::Et5406APlus)
        .unwrap();
    let Command::ConfigureBattery {
        battery,
        protections,
        ranges,
        voltage_thresholds,
    } = controls.configure_command()
    else {
        panic!("battery view must configure a battery profile");
    };
    assert_eq!(battery, controls.battery_draft);
    assert_eq!(protections, controls.protections);
    assert_eq!(ranges, controls.ranges);
    assert_eq!(voltage_thresholds, controls.voltage_thresholds);

    for battery_view in [false, true] {
        controls.battery_view = battery_view;
        controls.voltage_thresholds.off = f64::NAN;
        assert!(
            controls
                .validate_draft(Model::Et5406APlus)
                .is_err()
        );
    }
    controls.voltage_thresholds.off = 0.1;
    controls.battery_draft.current = 0.;
    assert!(
        controls
            .validate_draft(Model::Et5406APlus)
            .is_err()
    );
    controls.battery_view = false;
    controls
        .validate_draft(Model::Et5406APlus)
        .unwrap();
}
