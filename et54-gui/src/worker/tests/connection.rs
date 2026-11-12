use super::*;
use et54::transport;
use et54::{LoadMode, Model, VoltageThresholds};

#[test]
fn final_unlock_bypasses_cancellation_and_delays_and_closes_even_on_write_error() {
    for (termination, write_timeout) in [
        (b"\n".as_slice(), Duration::from_secs(2)),
        (b"\r\n".as_slice(), Duration::from_millis(10)),
        (b"\r".as_slice(), Duration::from_secs(2)),
    ] {
        for write_fails in [false, true] {
            let mut command = b"SYST:LOCA".to_vec();
            command.extend(termination);
            let step = if write_fails {
                transport::MockStep::WriteError(std::io::ErrorKind::TimedOut)
            } else {
                transport::MockStep::Write(command)
            };
            // No response is provided: any ACK read is an unexpected operation.
            let mock = Arc::new(Mutex::new(MockTransport::new([step])));
            let timeouts = Arc::new(Mutex::new(Vec::new()));
            let mut transport = Cancellable::new(
                TrackedTransport {
                    mock: mock.clone(),
                    timeouts: timeouts.clone(),
                },
                Arc::new(AtomicBool::new(true)),
                &ConnectionConfig {
                    write_termination: termination.to_vec(),
                    write_timeout,
                    operation_delay: Duration::from_secs(10),
                    ..Default::default()
                },
            );
            assert!(
                transport
                    .write_all(b"SYST:BEEP\n")
                    .is_err()
            );
            let now = Instant::now();
            assert_eq!(transport.close().is_err(), write_fails);
            transport.close().unwrap();
            assert!(now.elapsed() < Duration::from_millis(500), "unexpected close delay");
            let mock = mock.lock().unwrap();
            mock.assert_finished();
            assert!(mock.is_closed());
            assert_eq!(*timeouts.lock().unwrap(), vec![write_timeout.min(Duration::from_millis(100))]);
        }
    }
}

#[test]
fn reconnect_after_final_unlock_accepts_its_unread_ack_before_identity() {
    let mut observed = Vec::new();
    for reconnect in [false, true] {
        let mut steps = Vec::new();
        for (command, response) in CONNECTED {
            steps.push(transport::MockStep::Write(format!("{command}\n").into_bytes()));
            if reconnect && *command == "*IDN?" {
                steps.push(transport::MockStep::Read(b"Rexecu success\r\n".to_vec()));
            }
            steps.push(transport::MockStep::Read(format!("{response}\r\n").into_bytes()));
        }
        steps.push(transport::MockStep::Write(b"SYST:LOCA\n".to_vec()));
        observed.push(Arc::new(Mutex::new(MockTransport::new(steps))));
    }
    let mut mocks = observed.clone().into_iter();
    let worker = Worker::spawn(egui::Context::default(), move |_, mut c, stop| {
        c.operation_delay = Duration::ZERO;
        let transport = TrackedTransport {
            mock: mocks.next().ok_or(Error::Unavailable)?,
            timeouts: Arc::new(Mutex::new(Vec::new())),
        };
        Et54::connect(Cancellable::new(transport, stop, &c), c)
    })
    .unwrap();
    for id in [1, 3] {
        worker
            .send(id, Command::Connect("mock".into(), ConnectionConfig::default()))
            .unwrap();
        let connected = wait(&worker, id);
        assert!(connected.connected && connected.error.is_none());
        assert!(connected.has_current_settings());
        assert_eq!(connected.info.unwrap().model, Model::Et5406APlus);
        worker
            .send(id + 1, Command::Disconnect)
            .unwrap();
        let disconnected = wait(&worker, id + 1);
        assert!(!disconnected.connected && disconnected.error.is_none());
    }
    drop(worker);
    for mock in observed {
        let mock = mock.lock().unwrap();
        mock.assert_finished();
        assert!(mock.is_closed());
    }
}

#[test]
fn disconnect_and_worker_drop_send_one_final_unlock_without_reading_its_response() {
    for shutdown in [false, true] {
        let mut steps = Vec::new();
        for (command, response) in CONNECTED {
            steps.push(transport::MockStep::Write(format!("{command}\n").into_bytes()));
            steps.push(transport::MockStep::Read(format!("{response}\r\n").into_bytes()));
        }
        steps.push(transport::MockStep::Write(b"SYST:LOCA\n".to_vec()));
        let mock = Arc::new(Mutex::new(MockTransport::new(steps)));
        let timeouts = Arc::new(Mutex::new(Vec::new()));
        let observed = mock.clone();
        let recorded_timeouts = timeouts.clone();
        let worker = Worker::spawn(egui::Context::default(), move |_, mut c, stop| {
            c.operation_delay = Duration::ZERO;
            let transport = TrackedTransport {
                mock: mock.clone(),
                timeouts: timeouts.clone(),
            };
            Et54::connect(Cancellable::new(transport, stop, &c), c)
        })
        .unwrap();
        worker
            .send(1, Command::Connect("mock".into(), ConnectionConfig::default()))
            .unwrap();
        assert!(wait(&worker, 1).connected);
        if !shutdown {
            worker
                .send(2, Command::Disconnect)
                .unwrap();
            let state = wait(&worker, 2);
            assert!(!state.connected && state.error.is_none());
            assert_eq!(state.message, Text::DisconnectedUnlocked.into());
            assert_eq!(state.control, ControlState::Unknown);
        }
        drop(worker);
        let mock = observed.lock().unwrap();
        mock.assert_finished();
        assert!(mock.is_closed());
        assert_eq!(recorded_timeouts.lock().unwrap().last(), Some(&Duration::from_millis(100)));
    }
}

#[test]
fn connect_has_no_activation_and_worker_delivers_actual_readbacks() {
    let mut mock = Some(MockTransport::pairs(CONNECTED));
    let worker = Worker::spawn(egui::Context::default(), move |_, mut config, _| {
        // Simulate a slow open without blocking the caller/UI.
        thread::sleep(Duration::from_millis(100));
        config.operation_delay = Duration::ZERO;
        Et54::connect(mock.take().ok_or(Error::Unavailable)?, config)
    })
    .unwrap();
    let now = Instant::now();
    worker
        .send(1, Command::Connect("mock".into(), ConnectionConfig::default()))
        .unwrap();
    assert!(now.elapsed() < Duration::from_millis(50));
    let state = wait(&worker, 1);
    assert!(state.connected);
    assert_eq!(state.last_connected_port.as_deref(), Some("mock"));
    assert_eq!(state.status.unwrap().input, InputState::Off);
    assert_eq!(state.settings, Some(LoadMode::ConstantCurrent(1.)));
    assert_eq!(state.basic_setpoints, Some([1., 12., 20., 10.]));
    assert_eq!(state.protections.unwrap().voltage, 125.);
    assert_eq!(state.protections.unwrap().current, 21.);
    assert_eq!(state.voltage_thresholds, Some(VoltageThresholds { on: 1.5, off: 1. }));
    assert_eq!(state.control, ControlState::Remote);
    validate_gui_settings(
        state.info.as_ref().unwrap().model,
        state.settings.unwrap(),
        state.protections.unwrap(),
        state.ranges.unwrap(),
        state.voltage_thresholds.unwrap(),
    )
    .unwrap();
    worker
        .send(2, Command::Disconnect)
        .unwrap();
    let state = wait(&worker, 2);
    assert!(!state.connected);
    assert!(state.status.is_none());
    assert!(state.basic_setpoints.is_none());
    assert_eq!(state.last_connected_port.as_deref(), Some("mock"));
    worker
        .send(3, Command::Connect("unavailable".into(), ConnectionConfig::default()))
        .unwrap();
    let state = wait(&worker, 3);
    assert!(!state.connected && state.error.is_some());
    assert_eq!(state.last_connected_port.as_deref(), Some("mock"));
}
