//! Device worker with a bounded command queue and coalesced latest-state events.
//! No device or port-discovery operation is performed by the UI thread.
mod commands;
mod readback;
mod state;
mod transport;

use readback::poll_channel_measurements;
use state::clear_device;
pub(crate) use state::{BatteryReadback, BatterySample, BatterySettings, BatteryState, Command, ControlState, Sample, Snapshot, validate_gui_settings};
use transport::Cancellable;

use crate::i18n::{Message, Text};
use eframe::egui;
use et54::{ConnectionConfig, Et54, Result, SerialTransport, Transport};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Worker {
    tx: SyncSender<(u64, Command)>,
    pub state: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(ctx: egui::Context) -> std::io::Result<Self> {
        Self::spawn(ctx, |path, config, stop| {
            let transport = SerialTransport::open(path, &config)?;
            Et54::connect(Cancellable::new(transport, stop, &config), config)
        })
    }
    fn spawn<T, F>(ctx: egui::Context, mut connect: F) -> std::io::Result<Self>
    where
        T: Transport + 'static,
        F: FnMut(&str, ConnectionConfig, Arc<AtomicBool>) -> Result<Et54<T>> + Send + 'static,
    {
        let (tx, rx) = mpsc::sync_channel(8);
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let shared = state.clone();
        let stopping = stop.clone();
        let join = thread::Builder::new()
            .name("et54-device".into())
            .spawn(move || run_command_loop(rx, shared, stopping, ctx, &mut connect))?;
        Ok(Self {
            tx,
            state,
            stop,
            join: Some(join),
        })
    }
    pub fn send(&self, id: u64, command: Command) -> std::result::Result<(), Message> {
        self.tx
            .try_send((id, command))
            .map_err(|e| match e {
                TrySendError::Full(_) => Text::QueueFull.into(),
                TrySendError::Disconnected(_) => Text::WorkerStopped.into(),
            })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake an idle worker. Stop flag takes precedence over queued commands.
        let _ = self
            .tx
            .try_send((0, Command::Disconnect));
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn publish_snapshot(shared: &Mutex<Snapshot>, state: &Snapshot, ctx: &egui::Context) {
    if let Ok(mut target) = shared.lock() {
        *target = state.clone();
    }
    ctx.request_repaint();
}
fn run_command_loop<T, F>(rx: Receiver<(u64, Command)>, shared: Arc<Mutex<Snapshot>>, stop: Arc<AtomicBool>, ctx: egui::Context, connect: &mut F)
where
    T: Transport,
    F: FnMut(&str, ConnectionConfig, Arc<AtomicBool>) -> Result<Et54<T>>,
{
    let mut state = Snapshot::default();
    let mut device: Option<Et54<T>> = None;
    let mut interval = Duration::from_secs(1);
    let mut next = Instant::now();
    let mut sequence = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let wait = if device.is_some() && state.control == ControlState::Remote {
            next.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(100))
        } else {
            Duration::from_millis(100)
        };
        match rx.recv_timeout(wait) {
            Ok((id, command)) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                state.busy = true;
                state.error = None;
                publish_snapshot(&shared, &state, &ctx);
                commands::execute(command, &mut device, &mut state, &mut interval, &stop, connect);
                state.busy = false;
                state.completed = id;
                publish_snapshot(&shared, &state, &ctx);
                next = Instant::now() + interval;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if device.is_some() && state.control == ControlState::Remote && Instant::now() >= next {
                    // Polls are scheduled here, never queued by the repaint loop.
                    if let Some(d) = device.as_mut()
                        && let Err(e) = poll_channel_measurements(d, &mut state, &mut sequence)
                    {
                        state.error = Some(Message::Error {
                            summary: Text::MeasurementFailed,
                            detail: e.to_string(),
                        });
                        device.take();
                        clear_device(&mut state);
                        state.message = Text::ConnectionLost.into();
                    }
                    publish_snapshot(&shared, &state, &ctx);
                    next = Instant::now() + interval;
                }
            }
        }
    }
    if let Some(mut d) = device {
        let _ = d.close();
    }
}

#[cfg(test)]
mod tests;
