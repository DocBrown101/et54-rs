//! Shared mock transport and connection script for worker behavior tests.
use super::readback::refresh_channel_settings;
use super::*;
use crate::i18n::Language;
use et54::{Error, InputState, MockTransport, Mode};

mod battery;
mod commands;
mod connection;

struct TrackedTransport {
    mock: Arc<Mutex<MockTransport>>,
    timeouts: Arc<Mutex<Vec<Duration>>>,
}
impl Transport for TrackedTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.mock
            .lock()
            .unwrap()
            .write_all(bytes)
    }
    fn read(&mut self, bytes: &mut [u8]) -> Result<usize> {
        self.mock.lock().unwrap().read(bytes)
    }
    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.timeouts
            .lock()
            .unwrap()
            .push(timeout);
        self.mock
            .lock()
            .unwrap()
            .set_timeout(timeout)
    }
    fn close(&mut self) -> Result<()> {
        self.mock.lock().unwrap().close()
    }
}

const CONNECTED: &[(&str, &str)] = &[
    ("*IDN?", "ET5406A+ SN V1 H1"),
    ("Ch1:SW?", "OFF"),
    ("Ch1:MODE?", "CC"),
    ("LOAD1:ABNO?", "NONE"),
    ("Ch1:MODE?", "CC"),
    ("CURR1:CC?", "R1"),
    ("CURR1:CC?", "R1"),
    ("VOLT1:CV?", "R12"),
    ("POWE1:CP?", "R20"),
    ("RESI1:CR?", "R10"),
    ("VOLT1:VMAX?", "R125"),
    ("CURR1:IMAX?", "R21"),
    ("POWE1:PMAX?", "R85"),
    ("LOAD1:VRANGE?", "HIGH"),
    ("LOAD1:CRANGE?", "HIGH"),
    ("VOLT1:ON?", "R1.5"),
    ("VOLT1:OFF?", "R1"),
];

fn wait(worker: &Worker, id: u64) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let state = worker.state.lock().unwrap().clone();
        if state.completed == id {
            return state;
        }
        assert!(Instant::now() < deadline, "worker timeout");
        thread::sleep(Duration::from_millis(2));
    }
}

fn mock_worker(mock: MockTransport) -> (Worker, Arc<Mutex<MockTransport>>) {
    let mock = Arc::new(Mutex::new(mock));
    let observed = mock.clone();
    let worker = Worker::spawn(egui::Context::default(), move |_, mut c, _| {
        c.operation_delay = Duration::ZERO;
        Et54::connect(
            TrackedTransport {
                mock: mock.clone(),
                timeouts: Arc::new(Mutex::new(Vec::new())),
            },
            c,
        )
    })
    .unwrap();
    worker
        .send(1, Command::Connect("mock".into(), ConnectionConfig::default()))
        .unwrap();
    (worker, observed)
}
