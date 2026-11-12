//! Golden SCPI transcripts transcribed from reference channel.py and its tests.
//! No hardware is opened. Every script checks the full command and all returned data.
use et54::*;
use std::time::Duration;

fn device(script: &[(&str, &str)]) -> Et54<MockTransport> {
    let mut all = vec![("*IDN?", "ET5410A+ SN V1 H1")];
    all.extend_from_slice(script);
    Et54::connect(
        MockTransport::pairs(&all),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .expect("mock device should connect")
}

macro_rules! scalar_roundtrip {
    ($name:ident,$get:ident,$set:ident,$wire:literal,$value:literal) => {
        #[test]
        fn $name() {
            let query = format!("{}?", $wire);
            let write = format!("{} {}", $wire, $value);
            let response = format!("R{}", $value);
            let mut d = device(&[(&write, "Rexecu success"), (&query, &response)]);
            let mut ch = d.channel(1).unwrap();
            ch.$set($value).unwrap();
            assert_eq!(ch.$get().unwrap(), $value);
            d.transport().assert_finished();
        }
    };
}
scalar_roundtrip!(ovp, ovp, set_ovp, "VOLT1:VMAX", 24.5);
scalar_roundtrip!(ocp, ocp, set_ocp, "CURR1:IMAX", 4.0);
scalar_roundtrip!(opp, opp, set_opp, "POWE1:PMAX", 85.0);
scalar_roundtrip!(cc, cc_current, set_cc_current, "CURR1:CC", 3.1);
scalar_roundtrip!(cv, cv_voltage, set_cv_voltage, "VOLT1:CV", 12.5);
scalar_roundtrip!(cp, cp_power, set_cp_power, "POWE1:CP", 50.0);
scalar_roundtrip!(cr, cr_resistance, set_cr_resistance, "RESI1:CR", 100.0);
scalar_roundtrip!(cccv_current, cccv_current, set_cccv_current, "CURR1:CCCV", 2.5);
scalar_roundtrip!(cccv_voltage, cccv_voltage, set_cccv_voltage, "VOLT1:CCCV", 13.5);
scalar_roundtrip!(crcv_resistance, crcv_resistance, set_crcv_resistance, "RESI1:CRCV", 100.0);
scalar_roundtrip!(crcv_voltage, crcv_voltage, set_crcv_voltage, "VOLT1:CRCV", 13.5);
scalar_roundtrip!(led_current, led_current, set_led_current, "CURR1:LED", 0.8);
scalar_roundtrip!(led_voltage, led_voltage, set_led_voltage, "VOLT1:LED", 8.1);
scalar_roundtrip!(led_coefficient, led_coefficient, set_led_coefficient, "LED1:COEF", 0.2);
scalar_roundtrip!(battery_resistance, battery_resistance, set_battery_resistance, "RESI1:BCR", 500.0);

#[test]
fn all_mode_helpers_and_bug_fixes() {
    let modes = [
        (LoadMode::ConstantVoltage(12.), vec!["VOLT1:CV 12", "Ch1:MODE CV"]),
        (LoadMode::ConstantPower(50.), vec!["POWE1:CP 50", "Ch1:MODE CP"]),
        (LoadMode::ConstantResistance(100.), vec!["RESI1:CR 100", "Ch1:MODE CR"]),
        (
            LoadMode::CcCv { current: 2., voltage: 12.5 },
            vec!["CURR1:CCCV 2", "VOLT1:CCCV 12.5", "Ch1:MODE CCCV"],
        ),
        (
            LoadMode::CrCv {
                resistance: 100.,
                voltage: 13.5,
            },
            vec!["VOLT1:CRCV 13.5", "RESI1:CRCV 100", "Ch1:MODE CRCV"],
        ),
        (LoadMode::ShortCircuit, vec!["Ch1:MODE SHOR"]),
        (
            LoadMode::Led {
                voltage: 8.1,
                current: 0.8,
                coefficient: 0.2,
            },
            vec!["VOLT1:LED 8.1", "CURR1:LED 0.8", "LED1:COEF 0.2", "Ch1:MODE LED"],
        ),
    ];
    for (mode, commands) in modes {
        let pairs: Vec<_> = commands
            .into_iter()
            .map(|c| (c, "Rexecu success"))
            .collect();
        let mut d = device(&pairs);
        d.channel(1)
            .unwrap()
            .set_mode(mode)
            .unwrap();
        d.transport().assert_finished();
    }
    let mut d = device(&[]);
    assert!(matches!(
        d.channel(1)
            .unwrap()
            .set_mode(LoadMode::Led {
                voltage: 8.,
                current: 0.8,
                coefficient: 2.
            }),
        Err(Error::Invalid(_))
    ));
    d.transport().assert_finished();
}

#[test]
fn every_channel_mode_select_and_read() {
    for mode in [
        Mode::Cc,
        Mode::Cv,
        Mode::Cp,
        Mode::Cr,
        Mode::CcCv,
        Mode::CrCv,
        Mode::Short,
        Mode::Led,
        Mode::Battery,
        Mode::Transient,
        Mode::List,
        Mode::Scan,
    ] {
        let write = format!("Ch1:MODE {mode}");
        let mut d = device(&[(&write, "Rexecu success"), ("Ch1:MODE?", mode.as_str())]);
        let mut ch = d.channel(1).unwrap();
        ch.select_mode(mode).unwrap();
        assert_eq!(ch.mode().unwrap(), mode);
        d.transport().assert_finished();
    }
}

#[test]
fn ranges_trigger_qualification_and_status() {
    for range in [Range::Low, Range::High] {
        let v = format!("LOAD1:VRANGE {range}");
        let c = format!("LOAD1:CRANGE {range}");
        let mut d = device(&[
            (&v, "Rexecu success"),
            ("LOAD1:VRANGE?", range.as_str()),
            (&c, "Rexecu success"),
            ("LOAD1:CRANGE?", range.as_str()),
        ]);
        let mut ch = d.channel(1).unwrap();
        ch.set_voltage_range(range).unwrap();
        assert_eq!(ch.voltage_range().unwrap(), range);
        ch.set_current_range(range).unwrap();
        assert_eq!(ch.current_range().unwrap(), range);
        d.transport().assert_finished();
    }
    for mode in [TriggerMode::Manual, TriggerMode::External, TriggerMode::Software] {
        let write = format!("LOAD1:TRIG {mode}");
        let mut d = device(&[(&write, "Rexecu success"), ("LOAD1:TRIG?", mode.as_str()), ("*TRG", "Rexecu success")]);
        let mut ch = d.channel(1).unwrap();
        ch.set_trigger_mode(mode).unwrap();
        assert_eq!(ch.trigger_mode().unwrap(), mode);
        ch.trigger().unwrap();
        d.transport().assert_finished();
    }
    for p in [
        Protection::None,
        Protection::OverVoltage,
        Protection::OverCurrent,
        Protection::OverPower,
        Protection::OverTemperature,
        Protection::ReverseVoltage,
        Protection::FanFailure,
        Protection::Unregulated,
        Protection::Failure,
    ] {
        let mut d = device(&[("Ch1:SW?", "OFF"), ("Ch1:MODE?", "CR"), ("LOAD1:ABNO?", p.as_str())]);
        assert_eq!(
            d.channel(1).unwrap().status().unwrap(),
            ChannelStatus {
                input: InputState::Off,
                mode: Mode::Cr,
                protection: p
            }
        );
        d.transport().assert_finished();
    }
}

#[test]
fn battery_cc_three_stages_and_readback() {
    let mut d = device(&[
        ("BATT1:MODE CC", "Rexecu success"),
        ("BATT1:BCUT V", "Rexecu success"),
        ("BATT1:BCUT?", "Voltage"),
        ("CURR1:BCC1 1.3", "Rexecu success"),
        ("CURR1:BCC2 0.97", "Rexecu success"),
        ("CURR1:BCC3 0.97", "Rexecu success"),
        ("BATT1:BCUT?", "Voltage"),
        ("BATT1:MODE?", "CC"),
        ("VOLT1:BCC1 4.1", "Rexecu success"),
        ("VOLT1:BCC2 3.7", "Rexecu success"),
        ("VOLT1:BCC3 3.7", "Rexecu success"),
        ("Ch1:MODE BATT", "Rexecu success"),
        ("BATT1:MODE?", "CC"),
        ("BATT1:BCUT?", "V"),
        ("BATT1:BCUT?", "Voltage"),
        ("CURR1:BCC1?", "R1.3"),
        ("CURR1:BCC2?", "R0.97"),
        ("CURR1:BCC3?", "R0.97"),
        ("BATT1:BCUT?", "Voltage"),
        ("BATT1:MODE?", "CC"),
        ("VOLT1:BCC1?", "R4.1"),
        ("VOLT1:BCC2?", "R3.7"),
        ("VOLT1:BCC3?", "R3.7"),
        ("BATT1:CAPA?", "R0.7"),
        ("BATT1:ENER?", "R4.2"),
        ("BATT1:BAEN 2", "Rexecu success"),
        ("BATT1:BAEN?", "R2"),
    ]);
    let c = BatteryConfig {
        submode: BatterySubmode::Cc,
        value: BatteryValues::Stages(scpi::expand_three(&[1.3, 0.97]).unwrap()),
        cutoff: BatteryCutoff::Voltage,
        cutoff_value: BatteryValues::Stages([4.1, 3.7, 3.7]),
    };
    let mut ch = d.channel(1).unwrap();
    ch.configure_battery(c).unwrap();
    assert_eq!(ch.battery_config().unwrap(), c);
    assert_eq!(ch.battery_capacity().unwrap(), 0.7);
    assert_eq!(ch.battery_energy().unwrap(), 4.2);
    ch.set_battery_cutoff_level(2).unwrap();
    assert_eq!(ch.battery_cutoff_level().unwrap(), 2);
    d.transport().assert_finished();
}

#[test]
fn battery_all_scalar_cutoffs_cc_and_cr() {
    for submode in [BatterySubmode::Cc, BatterySubmode::Cr] {
        for (cutoff, word, system, value) in [
            (BatteryCutoff::Time, "Time", "TIME1:BTT", 5.),
            (BatteryCutoff::Capacity, "Capacity", "BATT1:BTC", 0.7),
            (BatteryCutoff::Energy, "Energy", "BATT1:BTE", 0.6),
        ] {
            let mode = format!("BATT1:MODE {submode}");
            let cut = format!("BATT1:BCUT {}", cutoff.as_str());
            let setpoint = if submode == BatterySubmode::Cc { "CURR1:BCC" } else { "RESI1:BCR" };
            let set = format!("{setpoint} 2");
            let threshold = format!("{system} {value}");
            let query = format!("{system}?");
            let response = format!("R{value}");
            let mut script = vec![(mode.as_str(), "Rexecu success"), (cut.as_str(), "Rexecu success")];
            if submode == BatterySubmode::Cc {
                script.push(("BATT1:BCUT?", word));
            }
            script.extend([
                (set.as_str(), "Rexecu success"),
                ("BATT1:BCUT?", word),
                (threshold.as_str(), "Rexecu success"),
                ("Ch1:MODE BATT", "Rexecu success"),
                ("BATT1:BCUT?", word),
                (query.as_str(), response.as_str()),
            ]);
            let mut d = device(&script);
            let mut ch = d.channel(1).unwrap();
            ch.configure_battery(BatteryConfig {
                submode,
                value: BatteryValues::Single(2.),
                cutoff,
                cutoff_value: BatteryValues::Single(value),
            })
            .unwrap();
            assert_eq!(ch.battery_cutoff_value().unwrap(), BatteryValues::Single(value));
            d.transport().assert_finished();
        }
    }
}

#[test]
fn battery_cr_voltage_cutoff_uses_documented_bcr_register() {
    let mut d = device(&[
        ("BATT1:MODE CR", "Rexecu success"),
        ("BATT1:BCUT V", "Rexecu success"),
        ("RESI1:BCR 500", "Rexecu success"),
        ("BATT1:BCUT?", "Voltage"),
        ("BATT1:MODE?", "CR"),
        ("VOLT1:BCR 10.5", "Rexecu success"),
        ("Ch1:MODE BATT", "Rexecu success"),
        ("BATT1:BCUT?", "Voltage"),
        ("BATT1:MODE?", "CR"),
        ("VOLT1:BCR?", "R10.5"),
    ]);
    let mut ch = d.channel(1).unwrap();
    ch.configure_battery(BatteryConfig {
        submode: BatterySubmode::Cr,
        value: BatteryValues::Single(500.),
        cutoff: BatteryCutoff::Voltage,
        cutoff_value: BatteryValues::Single(10.5),
    })
    .unwrap();
    assert_eq!(ch.battery_cutoff_value().unwrap(), BatteryValues::Single(10.5));
    d.transport().assert_finished();
}

#[test]
fn transient_all_submodes_and_triggers() {
    for submode in [TransientSubmode::Cc, TransientSubmode::Cv] {
        for trigger in [TransientTrigger::Continuous, TransientTrigger::Pulse, TransientTrigger::Trigger] {
            let system = if submode == TransientSubmode::Cc { "CURR1" } else { "VOLT1" };
            let mode = format!("TRAN1:STATE {submode}");
            let trg = format!("TRAN1:MODE {trigger}");
            let a = format!("{system}:TA 2.2");
            let b = format!("{system}:TB 8.1");
            let aq = format!("{system}:TA?");
            let bq = format!("{system}:TB?");
            let mut d = device(&[
                (&mode, "Rexecu success"),
                (&trg, "Rexecu success"),
                (&a, "Rexecu success"),
                (&b, "Rexecu success"),
                ("TIME1:WA 100", "Rexecu success"),
                ("TIME1:WB 200", "Rexecu success"),
                ("Ch1:MODE TRAN", "Rexecu success"),
                ("TRAN1:STATE?", submode.as_str()),
                ("TRAN1:MODE?", trigger.as_str()),
                (&aq, "R2.2"),
                (&bq, "R8.1"),
                ("TIME1:WA?", "R100"),
                ("TIME1:WB?", "R200"),
            ]);
            let c = TransientConfig {
                submode,
                trigger,
                values: [2.2, 8.1],
                width_ms: [100, 200],
            };
            let mut ch = d.channel(1).unwrap();
            ch.configure_transient(c).unwrap();
            assert_eq!(ch.transient_config().unwrap(), c);
            d.transport().assert_finished();
        }
    }
}

#[test]
fn list_configuration_rows_and_final_results() {
    let mut d = device(&[
        ("LIST1:MODE AUTO", "Rexecu success"),
        ("LIST1:PARA 1,0,4.5,30,2,15,8.5", "Rexecu success"),
        ("LIST1:PARA 2,1,13.5,60,1,1,0.1", "Rexecu success"),
        ("Ch1:MODE LIST", "Rexecu success"),
        ("LIST1:NUM 2", "Rexecu success"),
        ("LIST1:NUM?", "R2"),
        ("LIST1:LOOP ON", "Rexecu success"),
        ("LIST1:LOOP?", "ON"),
        ("LIST1:MODE?", "AUTO"),
        ("LIST1:MODE TRIGGER", "Rexecu success"),
        ("LIST1:MODE?", "TRIGGER"),
        ("LIST1:PARA? 1,2", "R1,0,4.5,30,2,15,8.5\r\n2,1,13.5,60,1,1,0.1"),
        ("LIST1:NUM?", "R2"),
        ("LIST1:OUT? 1,2", "R1,0,4.5,1,14,9\r\nR2,1,13.5,2,1.2,0.05"),
    ]);
    let rows = [
        ListRow {
            number: 1,
            mode: ListMode::Cc,
            value: 4.5,
            delay_s: 30,
            comparison: Comparison::Voltage,
            max: 15.,
            min: 8.5,
        },
        ListRow {
            number: 2,
            mode: ListMode::Cv,
            value: 13.5,
            delay_s: 60,
            comparison: Comparison::Current,
            max: 1.,
            min: 0.1,
        },
    ];
    let mut ch = d.channel(1).unwrap();
    ch.configure_list(ListStepMode::Auto, &rows)
        .unwrap();
    ch.set_list_steps(2).unwrap();
    assert_eq!(ch.list_steps().unwrap(), 2);
    ch.set_list_loop(true).unwrap();
    assert!(ch.list_loop().unwrap());
    assert_eq!(ch.list_step_mode().unwrap(), ListStepMode::Auto);
    ch.set_list_step_mode(ListStepMode::Trigger)
        .unwrap();
    assert_eq!(ch.list_step_mode().unwrap(), ListStepMode::Trigger);
    let read = ch.list_rows_range(1, 2).unwrap();
    assert_eq!(read[0].value, Some(4.5));
    assert_eq!(read[1].min, Some(0.1));
    let out = ch.list_results().unwrap();
    assert_eq!(out[0].result, TestResult::Pass);
    assert_eq!(out[1].result, TestResult::Fail);
    assert_eq!(out[1].max, Some(1.2));
    d.transport().assert_finished();
}

#[test]
fn list_parser_unavailable_fields_and_malformed_indices() {
    let row = ListRowReadback::parse("R1,4,---,5,0,---,---").unwrap();
    assert_eq!(row.value, None);
    assert_eq!(row.mode, ListMode::Open);
    assert_eq!(
        ListResult::parse("1,5,---,0,---,---")
            .unwrap()
            .result,
        TestResult::None
    );
    for s in ["1,6,1,5,0,2,0", "1,0,1,5,8,2,0", "0,0,1,5,0,2,0", "1,0,1,5,0,2", "1,0,bad,5,0,2,0"] {
        assert!(ListRowReadback::parse(s).is_err(), "{s}");
    }
    assert!(ListResult::parse("1,0,1,3,2,0").is_err());
    assert!(ListResult::parse("1,0,3,1,0,3,1").is_err()); // ambiguous PDF example
}

#[test]
fn scan_configuration_and_all_implemented_getters() {
    for (submode, threshold, value, system) in [
        (ScanSubmode::Cc, ScanThreshold::Voltage, 1.5, "CURR1"),
        (ScanSubmode::Cv, ScanThreshold::MinimumVoltage, 2.5, "VOLT1"),
        (ScanSubmode::Cp, ScanThreshold::Voltage, 1.5, "POWE1"),
    ] {
        let config = ScanConfig {
            submode,
            threshold,
            threshold_value: Some(value),
            comparison: ScanComparison::Current,
            limits: [0.5, 1.2],
            start_end: [1., 5.],
            step: 0.5,
            step_delay_s: 5,
        };
        let mode = format!("SCAN1:TYPE {submode}");
        let th = format!("SCAN1:THTYPE {threshold}");
        let v = format!("VOLT1:{threshold} {value}");
        let vq = format!("VOLT1:{threshold}?");
        let vr = format!("R{value}");
        let low = format!("{system}:LOW 0.5");
        let high = format!("{system}:HIGH 1.2");
        let start = format!("{system}:START 1");
        let end = format!("{system}:END 5");
        let step = format!("{system}:STEP 0.5");
        let lowq = format!("{system}:LOW?");
        let highq = format!("{system}:HIGH?");
        let startq = format!("{system}:START?");
        let endq = format!("{system}:END?");
        let stepq = format!("{system}:STEP?");
        let mut d = device(&[
            (&mode, "Rexecu success"),
            (&th, "Rexecu success"),
            ("SCAN1:THTYPE?", threshold.as_str()),
            (&v, "Rexecu success"),
            ("SCAN1:COMPARE INCURR", "Rexecu success"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&low, "Rexecu success"),
            (&high, "Rexecu success"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&start, "Rexecu success"),
            (&end, "Rexecu success"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&step, "Rexecu success"),
            ("TIME1:STEP 5", "Rexecu success"),
            ("Ch1:MODE SCAN", "Rexecu success"),
            ("SCAN1:TYPE?", submode.as_str()),
            ("SCAN1:THTYPE?", threshold.as_str()),
            ("SCAN1:THTYPE?", threshold.as_str()),
            (&vq, &vr),
            ("SCAN1:COMPARE?", "INCURR"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&lowq, "R0.5"),
            (&highq, "R1.2"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&startq, "R1"),
            (&endq, "R5"),
            ("SCAN1:TYPE?", submode.as_str()),
            (&stepq, "R0.5"),
            ("TIME1:STEP?", "R5"),
        ]);
        let mut ch = d.channel(1).unwrap();
        ch.configure_scan(config).unwrap();
        assert_eq!(ch.scan_config().unwrap(), config);
        d.transport().assert_finished();
    }
    let mut d = device(&[("SCAN1:THTYPE?", "DROP"), ("SCAN1:THTYPE?", "DROP")]);
    let mut ch = d.channel(1).unwrap();
    assert!(matches!(ch.scan_threshold_value(), Err(Error::Unsupported(_))));
    assert!(matches!(ch.set_scan_threshold_value(1.), Err(Error::Unsupported(_))));
    d.transport().assert_finished();
}

#[test]
fn qualification_overlay_full_roundtrip() {
    let mut d = device(&[
        ("QUAL1:VLOW 4", "Rexecu success"),
        ("QUAL1:VHIGH 12", "Rexecu success"),
        ("QUAL1:CLOW 1", "Rexecu success"),
        ("QUAL1:CHIGH 1.5", "Rexecu success"),
        ("QUAL1:PLOW 4.1", "Rexecu success"),
        ("QUAL1:PHIGH 13", "Rexecu success"),
        ("QUAL1:TEST ON", "Rexecu success"),
        ("QUAL1:TEST?", "ON"),
        ("QUAL1:VLOW?", "R4"),
        ("QUAL1:VHIGH?", "R12"),
        ("QUAL1:CLOW?", "R1"),
        ("QUAL1:CHIGH?", "R1.5"),
        ("QUAL1:PLOW?", "R4.1"),
        ("QUAL1:PHIGH?", "R13"),
        ("QUAL1:OUT?", "NONE"),
        ("QUAL1:TEST OFF", "Rexecu success"),
        ("QUAL1:TEST?", "OFF"),
    ]);
    let c = QualificationConfig {
        voltage: [4., 12.],
        current: [1., 1.5],
        power: [4.1, 13.],
    };
    let mut ch = d.channel(1).unwrap();
    ch.configure_qualification(c).unwrap();
    assert!(ch.qualification_enabled().unwrap());
    assert_eq!(ch.qualification_config().unwrap(), c);
    assert_eq!(ch.qualification_result().unwrap(), TestResult::None);
    ch.set_qualification_enabled(false)
        .unwrap();
    assert!(!ch.qualification_enabled().unwrap());
    d.transport().assert_finished();
}

#[test]
fn measurements_and_explicit_raw_device_operations() {
    let mut d = device(&[
        ("MEAS1:VOLTAGE?", "R12"),
        ("MEAS1:CURRENT?", "R1.5"),
        ("MEAS1:POWER?", "R18"),
        ("MEAS1:RESISTANCE?", "R8"),
        ("*TRG", "Rexecu success"),
        ("*IDN?", "ET5410A+ SN V1 H1"),
        ("*IDN?", "ET5410A+ SN V1 H1"),
    ]);
    let mut ch = d.channel(1).unwrap();
    assert_eq!(ch.read_voltage().unwrap(), 12.);
    assert_eq!(ch.read_current().unwrap(), 1.5);
    assert_eq!(ch.read_power().unwrap(), 18.);
    assert_eq!(ch.read_resistance().unwrap(), Some(8.));
    d.trigger().unwrap();
    d.lock().unwrap();
    assert_eq!(d.identify().unwrap().firmware, "V1");
    d.transport().assert_finished();
    use et54::transport::MockStep;
    let mut d = Et54::connect(
        MockTransport::new([
            MockStep::Write(b"*IDN?\n".to_vec()),
            MockStep::Read(b"ET5410 SN V1 H1\r\n".to_vec()),
            MockStep::Write(b"RST\n".to_vec()),
        ]),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .unwrap();
    d.reset().unwrap();
    assert!(!d.is_available());
    d.transport().assert_finished();
}

#[test]
fn compound_validation_happens_before_first_write() {
    let mut d = device(&[]);
    let mut ch = d.channel(1).unwrap();
    assert!(matches!(
        ch.configure_battery(BatteryConfig {
            submode: BatterySubmode::Cc,
            value: BatteryValues::Single(1.),
            cutoff: BatteryCutoff::Time,
            cutoff_value: BatteryValues::Single(0.5)
        }),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        ch.configure_transient(TransientConfig {
            submode: TransientSubmode::Cv,
            trigger: TransientTrigger::Continuous,
            values: [2., 8.],
            width_ms: [100, 49]
        }),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        ch.configure_qualification(QualificationConfig {
            voltage: [4., 12.],
            current: [2., 1.],
            power: [4., 13.]
        }),
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        ch.configure_list(
            ListStepMode::Auto,
            &[ListRow {
                number: 0,
                mode: ListMode::Cc,
                value: 1.,
                delay_s: 5,
                comparison: Comparison::Off,
                max: 0.,
                min: 0.
            }]
        ),
        Err(Error::Invalid(_))
    ));
    d.transport().assert_finished();
}

#[test]
fn voltage_thresholds_address_the_selected_channel() {
    let mut d = Et54::connect(
        MockTransport::pairs(&[
            ("*IDN?", "ET5420A+ SN V1 H1"),
            ("LOAD2:VRANGE?", "HIGH"),
            ("VOLT2:ON 12", "Rexecu success"),
            ("VOLT2:OFF 10", "Rexecu success"),
            ("VOLT2:ON?", "R12"),
            ("VOLT2:OFF?", "R10"),
        ]),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .unwrap();
    let thresholds = VoltageThresholds { on: 12., off: 10. };
    let mut ch = d.channel(2).unwrap();
    ch.set_voltage_thresholds(thresholds)
        .unwrap();
    assert_eq!(ch.voltage_thresholds().unwrap(), thresholds);
    d.transport().assert_finished();
}

#[test]
fn all_channel_switches_and_disable_all_continues_after_device_error() {
    let mut script = vec![("*IDN?".to_string(), "ET5420A+ SN V1 H1".to_string())];
    for n in 1..=2 {
        for (command, response) in [
            (format!("Ch{n}:MODE?"), "CC"),
            (format!("LOAD{n}:VRANGE?"), "HIGH"),
            (format!("LOAD{n}:CRANGE?"), "HIGH"),
            (format!("VOLT{n}:VMAX?"), "R24"),
            (format!("CURR{n}:IMAX?"), "R4"),
            (format!("POWE{n}:PMAX?"), "R85"),
            (format!("VOLT{n}:ON?"), "R1.5"),
            (format!("VOLT{n}:OFF?"), "R1"),
            (format!("LOAD{n}:ABNO?"), "NONE"),
            (format!("Ch{n}:MODE?"), "CC"),
            (format!("CURR{n}:CC?"), "R1"),
            (format!("Ch{n}:MODE?"), "CC"),
            (format!("Ch{n}:SW ON"), "Rexecu success"),
            (format!("Ch{n}:SW?"), "ON"),
        ] {
            script.push((command, response.to_string()));
        }
    }
    for (command, response) in [("Ch1:SW OFF", "Rexecu err"), ("Ch2:SW OFF", "Rexecu success"), ("Ch2:SW?", "OFF")] {
        script.push((command.into(), response.into()));
    }
    let pairs: Vec<_> = script
        .iter()
        .map(|(c, r)| (c.as_str(), r.as_str()))
        .collect();
    let mut d = Et54::connect(
        MockTransport::pairs(&pairs),
        ConnectionConfig {
            operation_delay: Duration::ZERO,
            ..Default::default()
        },
    )
    .unwrap();
    d.enable_all().unwrap();
    assert!(matches!(d.disable_all(), Err(Error::DisableFailed { channel: 1, .. })));
    d.transport().assert_finished();
}

#[test]
fn scalar_battery_current_readback_and_repeated_voltage_current() {
    let mut d = device(&[
        ("BATT1:BCUT?", "Time"),
        ("CURR1:BCC?", "R5.5"),
        ("BATT1:BCUT?", "Voltage"),
        ("CURR1:BCC1 1.25", "Rexecu success"),
        ("CURR1:BCC2 1.25", "Rexecu success"),
        ("CURR1:BCC3 1.25", "Rexecu success"),
    ]);
    let mut ch = d.channel(1).unwrap();
    assert_eq!(ch.battery_current().unwrap(), BatteryValues::Single(5.5));
    ch.set_battery_current(BatteryValues::Single(1.25))
        .unwrap();
    d.transport().assert_finished();
}

#[test]
fn all_ten_list_rows_and_batch_setter_are_checked() {
    let response = (1..=10)
        .map(|n| format!("R{n},0,1,5,0,0,0"))
        .collect::<Vec<_>>()
        .join("\r\n");
    let mut d = device(&[("LIST1:PARA? 1,10", &response), ("LIST1:PARA 1,0,1,5,0,0,0", "Rexecu success")]);
    let mut ch = d.channel(1).unwrap();
    let rows = ch.list_rows().unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[9].number, 10);
    let row = ListRow {
        number: 1,
        mode: ListMode::Cc,
        value: 1.,
        delay_s: 5,
        comparison: Comparison::Off,
        max: 0.,
        min: 0.,
    };
    ch.set_list_rows(&[row]).unwrap();
    assert!(matches!(ch.set_list_rows(&[row, row]), Err(Error::Invalid(_))));
    assert!(matches!(ch.configure_list(ListStepMode::Auto, &[row, row]), Err(Error::Invalid(_))));
    let invalid = ListRow { number: 0, ..row };
    for rows in [&[row, invalid][..], &[row; 11][..]] {
        assert!(matches!(ch.set_list_rows(rows), Err(Error::Invalid(_))));
        assert!(matches!(ch.configure_list(ListStepMode::Auto, rows), Err(Error::Invalid(_))));
    }
    d.transport().assert_finished();
}

#[test]
fn basic_setpoint_readbacks_do_not_depend_on_cached_gui_values() {
    for (mode, wire, expected) in [
        ("CV", "VOLT1:CV?", LoadMode::ConstantVoltage(2.)),
        ("CP", "POWE1:CP?", LoadMode::ConstantPower(2.)),
        ("CR", "RESI1:CR?", LoadMode::ConstantResistance(2.)),
    ] {
        let mut d = device(&[("Ch1:MODE?", mode), (wire, "R2")]);
        assert_eq!(
            d.channel(1)
                .unwrap()
                .load_mode()
                .unwrap(),
            expected
        );
        d.transport().assert_finished();
    }
    let mut d = device(&[
        ("Ch1:MODE?", "CCCV"),
        ("CURR1:CCCV?", "R2"),
        ("VOLT1:CCCV?", "R12"),
        ("Ch1:MODE?", "CRCV"),
        ("RESI1:CRCV?", "R100"),
        ("VOLT1:CRCV?", "R13.5"),
        ("Ch1:MODE?", "LED"),
        ("VOLT1:LED?", "R8.1"),
        ("CURR1:LED?", "R0.8"),
        ("LED1:COEF?", "R0.2"),
        ("Ch1:MODE?", "SHOR"),
    ]);
    let mut ch = d.channel(1).unwrap();
    assert_eq!(ch.load_mode().unwrap(), LoadMode::CcCv { current: 2., voltage: 12. });
    assert_eq!(
        ch.load_mode().unwrap(),
        LoadMode::CrCv {
            resistance: 100.,
            voltage: 13.5
        }
    );
    assert_eq!(
        ch.load_mode().unwrap(),
        LoadMode::Led {
            voltage: 8.1,
            current: 0.8,
            coefficient: 0.2
        }
    );
    assert_eq!(ch.load_mode().unwrap(), LoadMode::ShortCircuit);
    d.transport().assert_finished();
}

#[test]
fn every_list_mode_and_comparison_encoding_including_original_zero_volt_limit() {
    let mut commands = Vec::new();
    let mut rows = Vec::new();
    for (i, (mode, code)) in [
        (ListMode::Cc, 0),
        (ListMode::Cv, 1),
        (ListMode::Cp, 2),
        (ListMode::Cr, 3),
        (ListMode::Open, 4),
        (ListMode::Short, 5),
    ]
    .into_iter()
    .enumerate()
    {
        commands.push(format!("LIST1:PARA {},{code},1,5,0,0,0", i + 1));
        rows.push(ListRow {
            number: i as u8 + 1,
            mode,
            value: 1.,
            delay_s: 5,
            comparison: Comparison::Off,
            max: 0.,
            min: 0.,
        });
    }
    for (comparison, code) in [
        (Comparison::Current, 1),
        (Comparison::Voltage, 2),
        (Comparison::Power, 3),
        (Comparison::Resistance, 4),
    ] {
        commands.push(format!("LIST1:PARA 1,0,1,5,{code},13,1"));
        rows.push(ListRow {
            number: 1,
            mode: ListMode::Cc,
            value: 1.,
            delay_s: 5,
            comparison,
            max: 13.,
            min: 1.,
        });
    }
    // Exact fourth row from ET54_test.py:testLISTmode_list: comparison 0 V is valid,
    // even though a CV regulation setpoint has the distinct minimum 0.1 V.
    commands.push("LIST1:PARA 4,5,18.5,45,2,1,0".into());
    rows.push(ListRow {
        number: 4,
        mode: ListMode::Short,
        value: 18.5,
        delay_s: 45,
        comparison: Comparison::Voltage,
        max: 1.,
        min: 0.,
    });
    let pairs: Vec<_> = commands
        .iter()
        .map(|c| (c.as_str(), "Rexecu success"))
        .collect();
    let mut d = device(&pairs);
    let mut ch = d.channel(1).unwrap();
    for row in rows {
        ch.set_list_row(row).unwrap();
    }
    assert!(matches!(
        ch.set_list_row(ListRow {
            number: 1,
            mode: ListMode::Short,
            value: 1e100,
            delay_s: 5,
            comparison: Comparison::Off,
            max: 0.,
            min: 0.
        }),
        Err(Error::Invalid(_))
    ));
    d.transport().assert_finished();
}
