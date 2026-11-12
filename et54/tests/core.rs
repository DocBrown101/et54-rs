use et54::{scpi, transport::MockStep, *};
use std::{io::ErrorKind, time::Duration};

fn config() -> ConnectionConfig {
    ConnectionConfig {
        operation_delay: Duration::ZERO,
        ..Default::default()
    }
}
fn device(pairs: &[(&str, &str)]) -> Et54<MockTransport> {
    Et54::connect(MockTransport::pairs(pairs), config()).expect("mock device should connect")
}

#[test]
fn numeric_parsing_regressions() {
    assert_eq!(scpi::number(" R1.25E-2 ").unwrap(), 0.0125);
    assert_eq!(scpi::integer("R12").unwrap(), 12);
    for text in ["R1.5 12 18 8", "1.5 12 18 8", "R1.5,12,18,8", "1.5, 12, 18, 8"] {
        assert_eq!(scpi::numbers(text).unwrap(), vec![1.5, 12., 18., 8.]);
        let m = Measurements::parse(text).unwrap();
        assert_eq!((m.current, m.voltage, m.power, m.resistance), (1.5, 12., 18., Some(8.)));
    }
    assert_eq!(
        Measurements::parse("R0 12 0 OL")
            .unwrap()
            .resistance,
        None
    );
    for text in ["", "R", "RNaN", "inf", "abc", "1 V"] {
        assert!(scpi::number(text).is_err(), "{text}");
    }
    for text in ["1 2 3", "1,2,3,4,5", "1,,2,3,4", "1,2,3,junk"] {
        assert!(Measurements::parse(text).is_err(), "{text}");
    }
    assert_eq!(scpi::expand_three(&[1.3, 0.97]).unwrap(), [1.3, 0.97, 0.97]);
    assert_eq!(scpi::expand_three(&[1.25]).unwrap(), [1.25; 3]);
    assert!(scpi::expand_three(&[]).is_err());
    assert!(scpi::expand_three(&[1.; 4]).is_err());
}

#[test]
fn all_models_identity_channels_and_mustool() {
    for model in Model::ALL {
        let identity = format!("{model} SN123 V2 H1");
        let mut d = device(&[("*IDN?", &identity)]);
        assert_eq!(d.info().model, model);
        assert_eq!(d.info().serial_number.as_deref(), Some("SN123"));
        assert!(d.channel(0).is_err());
        assert!(
            d.channel(model.channel_count() + 1)
                .is_err()
        );
        assert_eq!(
            d.channel(model.channel_count())
                .unwrap()
                .number(),
            model.channel_count()
        );
        d.transport().assert_finished();
    }
    let info = DeviceInfo::parse("xxxxxx,V2,H1", Some(Model::Et5410APlus)).unwrap();
    assert_eq!(info.serial_number, None);
    assert_eq!(info.model, Model::Et5410APlus);
    assert!(DeviceInfo::parse("xxxxxx V2 H1", None).is_err());
    assert!(DeviceInfo::parse("ET9999 SN V2 H1", None).is_err());
    assert!(DeviceInfo::parse("ET5410A+ SN", None).is_err());
    assert!(DeviceInfo::parse("ET5410A+,,SN,V1,H1", None).is_err());
    assert_eq!(
        DeviceInfo::parse("et5406a+,SN,V1,H1", None)
            .unwrap()
            .model,
        Model::Et5406APlus
    );
}

#[test]
fn initial_identity_skips_stale_success_acks_without_weakening_later_queries() {
    for acknowledgements in [0, 1, 3] {
        let response = format!("{}ET5406A+ SN V1 H1", "Rexecu success\r\n".repeat(acknowledgements));
        let mut d = device(&[
            ("*IDN?", &response),
            ("SELF:FAN?", "PASS"),
            ("MEAS1:VOLTAGE?", "Rexecu success"),
            ("*IDN?", "Rexecu success"),
        ]);
        assert_eq!(d.info().model, Model::Et5406APlus);
        assert_eq!(d.fan().unwrap(), FanStatus::Pass);
        assert!(matches!(d.channel(1).unwrap().read_voltage(), Err(Error::Protocol(_))));
        assert!(matches!(d.identify(), Err(Error::Protocol(_))));
        d.transport().assert_finished();
    }
}

#[test]
fn initial_identity_keeps_device_errors_and_invalid_responses_strict() {
    for response in ["Rcmd err", "Rexecu err", "garbage", "Rexecu success!"] {
        let response_with_ack = format!("Rexecu success\r\n{response}");
        let result = Et54::connect(MockTransport::pairs(&[("*IDN?", &response_with_ack)]), config());
        if response.ends_with("err") {
            assert!(matches!(result, Err(Error::Device { .. })));
        } else {
            assert!(matches!(result, Err(Error::Protocol(_))));
        }
    }
}

#[test]
fn initial_identity_times_out_if_only_acknowledgements_arrive() {
    let result = Et54::connect(
        MockTransport::new([
            MockStep::Write(b"*IDN?\n".to_vec()),
            MockStep::Read(b"Rexecu success\r\nRexecu success\r\n".to_vec()),
            MockStep::ReadError(ErrorKind::TimedOut),
        ]),
        config(),
    );
    assert!(matches!(result, Err(Error::Io(error)) if error.kind() == ErrorKind::TimedOut));
}

#[test]
fn scpi_exchanges_pause_once_per_response() {
    let delay = Duration::from_millis(200);
    let start = std::time::Instant::now();
    let mut d = Et54::connect(
        MockTransport::pairs(&[("*IDN?", "ET5406A+ SN V1 H1"), ("SYST:BEEP", "Rexecu success"), ("SELF:FAN?", "PASS")]),
        ConnectionConfig {
            operation_delay: delay,
            ..Default::default()
        },
    )
    .unwrap();
    d.beep().unwrap();
    assert_eq!(d.fan().unwrap(), FanStatus::Pass);
    let elapsed = start.elapsed();
    assert!(elapsed >= delay * 3, "missing response pauses: {elapsed:?}");
    // Allow 400 ms scheduling overhead; double pauses would take at least 1.2 s.
    assert!(elapsed < delay * 5, "extra SCPI pauses: {elapsed:?}");
    d.transport().assert_finished();
}

#[test]
fn initial_identity_acks_do_not_restart_the_read_timeout() {
    let result = Et54::connect(
        MockTransport::pairs(&[("*IDN?", "Rexecu success\r\nET5406A+ SN V1 H1")]),
        ConnectionConfig {
            read_timeout: Duration::from_millis(20),
            // Processing one ACK exhausts the common budget. The queued IDN must not be read.
            operation_delay: Duration::from_millis(40),
            ..config()
        },
    );
    assert!(matches!(result, Err(Error::Io(error)) if error.kind() == ErrorKind::TimedOut));
}

#[test]
fn connect_only_identifies_and_close_is_idempotent() {
    let mut d = device(&[("*IDN?", "ET5406A+ SN V1 H1")]);
    assert!(d.is_available());
    d.transport().assert_finished();
    d.close().unwrap();
    d.close().unwrap();
    assert!(d.transport().is_closed());
    assert!(matches!(d.query_raw("*IDN?"), Err(Error::Unavailable)));
}

#[test]
fn mock_remembers_unexpected_reads_and_writes_even_after_close() {
    for (steps, write) in [
        (vec![], true),
        (vec![], false),
        (vec![MockStep::Write(b"expected".to_vec())], true),
        (vec![MockStep::Read(b"expected".to_vec())], true),
        (vec![MockStep::Write(b"expected".to_vec())], false),
    ] {
        let mut mock = MockTransport::new(steps);
        let result = if write {
            mock.write_all(b"unexpected")
        } else {
            mock.read(&mut [0]).map(|_| ())
        };
        assert!(matches!(result, Err(Error::Mock(_))));
        mock.close().unwrap();
        assert!(std::panic::catch_unwind(|| mock.assert_finished()).is_err());
    }

    let mut mock = MockTransport::new([MockStep::Write(b"expected".to_vec()), MockStep::Write(b"next".to_vec())]);
    assert!(matches!(mock.write_all(b"wrong"), Err(Error::Mock(_))));
    mock.write_all(b"next").unwrap();
    assert!(std::panic::catch_unwind(|| mock.assert_finished()).is_err());
}

#[test]
fn acknowledgments_and_device_errors() {
    let mut d = device(&[
        ("*IDN?", "ET5410A+ SN V1 H1"),
        ("SYST:BEEP", "Rexecu success"),
        ("BAD", "Rcmd err"),
        ("BAD2", "Rexecu err"),
        ("SELF:FAN?", "PASS"),
        ("SYST:LOCA", "Rexecu success"),
    ]);
    d.beep().unwrap();
    assert!(matches!(d.write_raw("BAD"), Err(Error::Device { .. })));
    assert!(matches!(d.query_raw("BAD2"), Err(Error::Device { .. })));
    assert_eq!(d.fan().unwrap(), FanStatus::Pass);
    d.unlock().unwrap();
    d.transport().assert_finished();
    let mut d = device(&[("*IDN?", "ET5410 SN V1 H1"), ("SYST:BEEP", "OK")]);
    assert!(matches!(d.beep(), Err(Error::Protocol(_))));
    assert!(!d.is_available());
}

#[test]
fn transport_failure_poisoning_and_no_write_retry() {
    let steps = [
        MockStep::Write(b"*IDN?\n".to_vec()),
        MockStep::Read(b"ET5410 SN V1 H1\r\n".to_vec()),
        MockStep::Write(b"SYST:BEEP\n".to_vec()),
        MockStep::Read(b"Rexecu suc".to_vec()),
        MockStep::ReadError(ErrorKind::TimedOut),
    ];
    let mut d = Et54::connect(MockTransport::new(steps), config()).unwrap();
    assert!(matches!(d.beep(), Err(Error::Io(_))));
    assert!(matches!(d.beep(), Err(Error::Unavailable)));
    d.transport().assert_finished();
    let mut d = Et54::connect(
        MockTransport::new([
            MockStep::Write(b"*IDN?\n".to_vec()),
            MockStep::Read(b"ET5410 SN V1 H1\r\n".to_vec()),
            MockStep::WriteError(ErrorKind::BrokenPipe),
        ]),
        config(),
    )
    .unwrap();
    assert!(d.beep().is_err());
    assert!(!d.is_available());
    d.transport().assert_finished();
}

#[test]
fn fragmented_custom_terminators_empty_and_disconnect() {
    let mut c = config();
    c.write_termination = b"\r".to_vec();
    c.read_termination = b"\n".to_vec();
    let d = Et54::connect(
        MockTransport::new([
            MockStep::Write(b"*IDN?\r".to_vec()),
            MockStep::Read(b"ET54".to_vec()),
            MockStep::Read(b"06A+ SN V1 H1\n".to_vec()),
        ]),
        c,
    )
    .unwrap();
    assert_eq!(d.info().model, Model::Et5406APlus);
    d.transport().assert_finished();
    for bytes in [b"\r\n".to_vec(), vec![]] {
        assert!(Et54::connect(MockTransport::new([MockStep::Write(b"*IDN?\n".to_vec()), MockStep::Read(bytes)]), config()).is_err());
    }
}

#[test]
fn basic_mode_write_order_and_actual_readback() {
    let mut d = device(&[
        ("*IDN?", "ET5420A+ SN V1 H1"),
        ("CURR2:CC 1.5", "Rexecu success"),
        ("Ch2:MODE CC", "Rexecu success"),
        ("Ch2:MODE?", "CC"),
        ("CURR2:CC?", "R1.5"),
        ("MEAS2:ALL?", "R1.5 12 18 8"),
    ]);
    let mut ch = d.channel(2).unwrap();
    ch.set_mode(LoadMode::ConstantCurrent(1.5))
        .unwrap();
    assert_eq!(ch.load_mode().unwrap(), LoadMode::ConstantCurrent(1.5));
    assert_eq!(ch.measure_all().unwrap().power, 18.);
    d.transport().assert_finished();
}

#[test]
fn validation_and_capabilities_write_nothing() {
    let defaults = ConnectionConfig::default();
    assert_eq!(defaults.baud_rate, 9600);
    assert_eq!(defaults.operation_delay, Duration::from_millis(10));
    assert_eq!(defaults.read_timeout, Duration::from_secs(2));
    assert_eq!(defaults.write_timeout, Duration::from_secs(2));
    assert_eq!(defaults.list_timeout, Duration::from_millis(2500));
    assert_eq!(defaults.read_termination, b"\r\n");
    assert_eq!(defaults.write_termination, b"\n");
    assert!(
        ConnectionConfig {
            read_termination: vec![],
            ..config()
        }
        .validate()
        .is_err()
    );
    assert!(
        ConnectionConfig {
            write_timeout: Duration::ZERO,
            ..config()
        }
        .validate()
        .is_err()
    );
    let mut d = device(&[("*IDN?", "ET5406A+ SN V1 H1")]);
    let mut ch = d.channel(1).unwrap();
    for v in [-1., f64::NAN, f64::INFINITY, 21.] {
        assert!(matches!(ch.set_cc_current(v), Err(Error::Invalid(_))));
    }
    assert!(matches!(ch.set_cr_resistance(0.), Err(Error::Invalid(_))));
    assert!(matches!(ch.set_mode(LoadMode::CcCv { current: 1., voltage: 12. }), Err(Error::Unsupported(_))));
    assert!(matches!(
        ch.set_mode(LoadMode::Led {
            current: 1.,
            voltage: 12.,
            coefficient: 0.2
        }),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        ch.set_mode(LoadMode::CrCv {
            resistance: 100.,
            voltage: 12.
        }),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(ch.set_transient_width_ms([49, 100]), Err(Error::Invalid(_))));
    assert!(matches!(
        ch.set_protection_limits(ProtectionLimits {
            voltage: 12.,
            current: 1.,
            power: -1.
        }),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(d.write_raw("SYST:BEEP\nCh1:SW ON"), Err(Error::Invalid(_))));
    d.transport().assert_finished();
}

#[test]
fn et5406_protections_allow_device_thresholds_without_raising_setpoints() {
    let mut d = device(&[
        ("*IDN?", "ET5406A+ SN V1 H1"),
        ("VOLT1:VMAX 125", "Rexecu success"),
        ("VOLT1:CV 120", "Rexecu success"),
        ("CURR1:IMAX 21", "Rexecu success"),
        ("CURR1:CC 20", "Rexecu success"),
    ]);
    let mut ch = d.channel(1).unwrap();
    ch.set_ovp(125.).unwrap();
    ch.set_cv_voltage(120.).unwrap();
    ch.set_ocp(21.).unwrap();
    ch.set_cc_current(20.).unwrap();
    assert!(matches!(ch.set_ovp(125.1), Err(Error::Invalid(_))));
    assert!(matches!(ch.set_cv_voltage(120.1), Err(Error::Invalid(_))));
    assert!(matches!(ch.set_ocp(21.1), Err(Error::Invalid(_))));
    assert!(matches!(ch.set_cc_current(20.1), Err(Error::Invalid(_))));
    d.transport().assert_finished();
}

#[test]
fn voltage_thresholds_validate_both_values_before_writing_and_use_actual_range() {
    let mut d = device(&[
        ("*IDN?", "ET5406A+ SN V1 H1"),
        ("LOAD1:VRANGE?", "LOW"),
        ("LOAD1:VRANGE?", "LOW"),
        ("VOLT1:ON 20", "Rexecu success"),
        ("VOLT1:OFF 0.1", "Rexecu success"),
        ("VOLT1:ON?", "R2E1"),
        ("VOLT1:OFF?", "R0.1"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("VOLT1:ON 120", "Rexecu success"),
        ("VOLT1:OFF 120", "Rexecu success"),
    ]);
    let mut ch = d.channel(1).unwrap();
    for invalid in [0.09, 120.1, f64::NAN, f64::INFINITY] {
        for thresholds in [VoltageThresholds { on: invalid, off: 1. }, VoltageThresholds { on: 1., off: invalid }] {
            assert!(matches!(ch.set_voltage_thresholds(thresholds), Err(Error::Invalid(_))));
        }
    }
    assert!(matches!(
        ch.set_voltage_thresholds(VoltageThresholds { on: 1., off: 20.1 }),
        Err(Error::Invalid(_))
    ));
    let thresholds = VoltageThresholds { on: 20., off: 0.1 };
    ch.set_voltage_thresholds(thresholds)
        .unwrap();
    assert_eq!(ch.voltage_thresholds().unwrap(), thresholds);
    ch.set_voltage_thresholds(VoltageThresholds { on: 120., off: 120. })
        .unwrap();
    d.transport().assert_finished();
}

#[test]
fn activation_rejects_out_of_range_voltage_thresholds_without_enabling() {
    let mut d = device(&[
        ("*IDN?", "ET5406A+ SN V1 H1"),
        ("Ch1:MODE?", "CC"),
        ("LOAD1:VRANGE?", "LOW"),
        ("LOAD1:CRANGE?", "HIGH"),
        ("VOLT1:VMAX?", "R21"),
        ("CURR1:IMAX?", "R21"),
        ("POWE1:PMAX?", "R85"),
        ("VOLT1:ON?", "R20"),
        ("VOLT1:OFF?", "R20.1"),
    ]);
    assert!(matches!(d.channel(1).unwrap().enable(), Err(Error::Invalid(_))));
    d.transport().assert_finished();
}

#[test]
fn load_mode_validation_respects_model_ranges_and_protections() {
    for model in Model::ALL {
        let limits = model.limits();
        let p = ProtectionLimits {
            voltage: limits.ovp,
            current: limits.ocp,
            power: limits.opp,
        };
        for mode in [
            LoadMode::ConstantCurrent(limits.current),
            LoadMode::ConstantVoltage(limits.voltage),
            LoadMode::ConstantPower(limits.power),
            LoadMode::ConstantResistance(limits.resistance),
        ] {
            mode.validate(model, p, [Range::High; 2])
                .unwrap();
        }
        for mode in [
            LoadMode::ConstantCurrent(limits.current + 1.),
            LoadMode::ConstantVoltage(limits.voltage + 1.),
            LoadMode::ConstantPower(limits.power + 1.),
            LoadMode::ConstantResistance(limits.resistance + 1.),
            LoadMode::ConstantCurrent(f64::NAN),
        ] {
            assert!(matches!(mode.validate(model, p, [Range::High; 2]), Err(Error::Invalid(_))));
        }
        let combined = LoadMode::CcCv { current: 1., voltage: 12. };
        match combined.validate(model, p, [Range::High; 2]) {
            Ok(()) => assert!(model.supports(Mode::CcCv)),
            Err(Error::Unsupported(_)) => assert!(!model.supports(Mode::CcCv)),
            result => panic!("unexpected validation result: {result:?}"),
        }
    }

    let model = Model::Et5406APlus;
    let low = ProtectionLimits {
        voltage: 21.,
        current: 3.3,
        power: 200.,
    };
    for mode in [LoadMode::ConstantCurrent(3.), LoadMode::ConstantVoltage(20.)] {
        mode.validate(model, low, [Range::Low; 2])
            .unwrap();
    }
    for mode in [LoadMode::ConstantCurrent(3.01), LoadMode::ConstantVoltage(20.01)] {
        assert!(matches!(mode.validate(model, low, [Range::Low; 2]), Err(Error::Invalid(_))));
    }
    for p in [
        ProtectionLimits { voltage: 21.1, ..low },
        ProtectionLimits { current: 3.4, ..low },
        ProtectionLimits { current: 0., ..low },
        ProtectionLimits { power: 0., ..low },
        ProtectionLimits { voltage: f64::NAN, ..low },
    ] {
        assert!(matches!(
            LoadMode::ConstantCurrent(1.).validate(model, p, [Range::Low; 2]),
            Err(Error::Invalid(_))
        ));
    }
    assert!(matches!(
        LoadMode::ConstantCurrent(2.).validate(model, ProtectionLimits { current: 1., ..low }, [Range::High; 2],),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn activation_checks_real_mode_ranges_and_protections_before_on() {
    let mut d = device(&[
        ("*IDN?", "ET5406A+ SN V1 H1"),
        ("Ch1:MODE?", "CC"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("LOAD1:CRANGE?", "HIGH"),
        ("VOLT1:VMAX?", "R125"),
        ("CURR1:IMAX?", "R21"),
        ("POWE1:PMAX?", "R85"),
        ("VOLT1:ON?", "R1.5"),
        ("VOLT1:OFF?", "R1"),
        ("LOAD1:ABNO?", "NONE"),
        ("Ch1:MODE?", "CC"),
        ("CURR1:CC?", "R2"),
        ("Ch1:MODE?", "CC"),
        ("Ch1:SW ON", "Rexecu success"),
        ("Ch1:SW?", "ON"),
        ("Ch1:SW OFF", "Rexecu success"),
        ("Ch1:SW?", "OFF"),
    ]);
    d.channel(1).unwrap().enable().unwrap();
    d.channel(1).unwrap().disable().unwrap();
    d.transport().assert_finished();
    let mut d = device(&[
        ("*IDN?", "ET5410 SN V1 H1"),
        ("Ch1:MODE?", "CC"),
        ("LOAD1:VRANGE?", "HIGH"),
        ("LOAD1:CRANGE?", "HIGH"),
        ("VOLT1:VMAX?", "R24"),
        ("CURR1:IMAX?", "R1"),
        ("POWE1:PMAX?", "R85"),
        ("VOLT1:ON?", "R1.5"),
        ("VOLT1:OFF?", "R1"),
        ("LOAD1:ABNO?", "NONE"),
        ("Ch1:MODE?", "CC"),
        ("CURR1:CC?", "R2"),
    ]);
    assert!(matches!(d.channel(1).unwrap().enable(), Err(Error::Invalid(_))));
    d.transport().assert_finished();
}

#[test]
fn failed_disable_never_claims_off() {
    let mut d = device(&[("*IDN?", "ET5410 SN V1 H1"), ("Ch1:SW OFF", "Rexecu err")]);
    let err = d
        .channel(1)
        .unwrap()
        .disable()
        .unwrap_err();
    assert!(matches!(err, Error::DisableFailed { channel: 1, .. }));
    assert!(
        err.to_string()
            .contains("INPUT STATE UNKNOWN")
    );
    d.transport().assert_finished();
}

#[test]
fn timeout_override_does_not_change_default() {
    let mut d = device(&[
        ("*IDN?", "ET5410 SN V1 H1"),
        ("LIST1:PARA? 1,2", "R1,0,1,5,0,2,0\r\nR2,1,12,5,0,20,1"),
        ("SELF:FAN?", "PASS"),
    ]);
    let rows = d
        .channel(1)
        .unwrap()
        .list_rows_range(1, 2)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].mode, ListMode::Cv);
    assert_eq!(d.fan().unwrap(), FanStatus::Pass);
    d.transport().assert_finished();
}
