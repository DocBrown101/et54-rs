//! Exercise the real serialport/SerialTransport path against a Linux pseudo-terminal.
//! No physical hardware, load activation or permission to a USB device is required.
#![cfg(target_os = "linux")]
use et54::*;
use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
fn reopening_clears_queued_input_and_initial_identity_accepts_a_late_unlock_ack() {
    let pty = nix::pty::openpty(None, None).unwrap();
    let path = nix::unistd::ttyname(&pty.slave).unwrap();
    let config = ConnectionConfig {
        operation_delay: Duration::ZERO,
        read_timeout: Duration::from_millis(500),
        ..Default::default()
    };
    // Establish raw mode before queueing a previous connection's unread bytes.
    SerialTransport::open(path.to_str().unwrap(), &config)
        .unwrap()
        .close()
        .unwrap();
    let mut master = File::from(pty.master);
    master
        .write_all(b"Rexecu success\r\nstale partial frame")
        .unwrap();
    let port = SerialTransport::open(path.to_str().unwrap(), &config).unwrap();
    let (done, finished) = mpsc::channel();
    let peer = thread::spawn(move || {
        let mut master = BufReader::new(master);
        let _slave = pty.slave;
        for (command, fragments) in [
            ("*IDN?", vec!["Rexecu ", "success\r\n", "ET5406A+ SN V1 H1\r\n"]),
            ("SELF:FAN?", vec!["PASS\r\n"]),
        ] {
            let mut received = Vec::new();
            master
                .read_until(b'\n', &mut received)
                .unwrap();
            assert_eq!(received, format!("{command}\n").as_bytes());
            for fragment in fragments {
                thread::sleep(Duration::from_millis(5));
                master
                    .get_mut()
                    .write_all(fragment.as_bytes())
                    .unwrap();
            }
        }
        // Keep the peer alive until the final response has been consumed.
        let _ = finished.recv();
    });
    let mut device = Et54::connect(port, config).unwrap();
    assert_eq!(device.info().model, Model::Et5406APlus);
    assert_eq!(device.fan().unwrap(), FanStatus::Pass);
    device.close().unwrap();
    done.send(()).unwrap();
    peer.join().unwrap();
}

#[test]
fn serial_transport_fragmentation_multiline_and_timeout() {
    let pty = nix::pty::openpty(None, None).unwrap();
    let path = nix::unistd::ttyname(&pty.slave).unwrap();
    let config = ConnectionConfig {
        operation_delay: Duration::ZERO,
        read_timeout: Duration::from_millis(150),
        ..Default::default()
    };
    // Open before the peer starts so that the PTY has raw serial settings (no echo).
    let port = SerialTransport::open(path.to_str().unwrap(), &config).unwrap();
    let peer = thread::spawn(move || {
        let mut master = BufReader::new(File::from(pty.master));
        let _slave = pty.slave;
        for (command, fragments) in [
            ("*IDN?", vec!["ET54", "20A+ SN V1 H1\r", "\n"]),
            ("LIST2:PARA? 1,2", vec!["R1,0,1,5,0,2,0\r\n", "R2,1,12,5,0,20,1\r\n"]),
            ("MEAS2:ALL?", vec!["R1.5,12,18,8\r\n"]),
            ("SELF:FAN?", vec!["PA"]), // deliberately unterminated response
        ] {
            let mut received = Vec::new();
            master
                .read_until(b'\n', &mut received)
                .unwrap();
            assert_eq!(received, format!("{command}\n").as_bytes());
            for bytes in fragments {
                master
                    .get_mut()
                    .write_all(bytes.as_bytes())
                    .unwrap();
                thread::sleep(Duration::from_millis(3));
            }
        }
        // Keep the PTY open long enough to test a real OS timeout rather than EOF.
        thread::sleep(Duration::from_millis(250));
    });
    let mut device = Et54::connect(port, config).unwrap();
    assert_eq!(device.channel_count(), 2);
    let rows = device
        .channel(2)
        .unwrap()
        .list_rows_range(1, 2)
        .unwrap();
    assert_eq!(rows[1].value, Some(12.));
    assert_eq!(
        device
            .channel(2)
            .unwrap()
            .measure_all()
            .unwrap()
            .current,
        1.5
    );
    assert!(matches!(device.fan(), Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::TimedOut));
    assert!(!device.is_available());
    device.close().unwrap();
    peer.join().unwrap();
}
