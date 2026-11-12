//! Connects the dashboard to the worker and handles the application lifecycle.
use crate::i18n::{Language, Message};
use crate::{
    ui,
    worker::{Command, Snapshot, Worker},
};
use eframe::egui;
use std::time::Duration;

const LAST_CONNECTED_PORT_KEY: &str = "last_connected_port";
const LANGUAGE_KEY: &str = "language";

pub struct Et54App {
    smoke_frames: Option<u8>,
    worker: Worker,
    state: Snapshot,
    ui: ui::Dashboard,
    last_connected_port: String,
    next_id: u64,
    pending: Option<u64>,
    local_error: Option<Message>,
}
impl Et54App {
    pub fn new(cc: &eframe::CreationContext<'_>, smoke_test: bool) -> std::io::Result<Self> {
        ui::configure_ui_style(&cc.egui_ctx);
        let mut app = Self::with_worker(Worker::start(cc.egui_ctx.clone())?, cc.storage);
        app.smoke_frames = smoke_test.then_some(3);
        Ok(app)
    }
    fn with_worker(worker: Worker, storage: Option<&dyn eframe::Storage>) -> Self {
        let last_connected_port = storage
            .and_then(|s| s.get_string(LAST_CONNECTED_PORT_KEY))
            .unwrap_or_default();
        let mut app = Self {
            smoke_frames: None,
            worker,
            state: Snapshot::default(),
            ui: ui::Dashboard::new(last_connected_port.clone()),
            last_connected_port,
            next_id: 1,
            pending: None,
            local_error: None,
        };
        app.ui.language = storage
            .and_then(|s| s.get_string(LANGUAGE_KEY))
            .map_or_else(Language::default, |code| Language::from_code(&code));
        app.send_command(Command::Ports);
        app
    }
    fn send_command(&mut self, command: Command) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        match self.worker.send(id, command) {
            Ok(()) => self.pending = Some(id),
            Err(e) => self.local_error = Some(e),
        }
    }
    fn sync_from_worker(&mut self) {
        if let Ok(state) = self.worker.state.try_lock() {
            self.state = state.clone();
        }
        if self
            .pending
            .is_some_and(|id| self.state.completed >= id)
        {
            self.pending = None;
        }
        self.ui
            .sync_from_snapshot(&self.state, self.pending.is_some());
    }
    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        let commands = self
            .ui
            .show(ui, &self.state, self.pending.is_some(), &mut self.local_error);
        for command in commands {
            self.send_command(command);
        }
    }
}
impl eframe::App for Et54App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.smoke_frames.is_some() {
            return;
        }
        if let Ok(state) = self.worker.state.lock()
            && let Some(port) = &state.last_connected_port
        {
            self.last_connected_port
                .clone_from(port);
        }
        storage.set_string(LAST_CONNECTED_PORT_KEY, self.last_connected_port.clone());
        storage.set_string(LANGUAGE_KEY, self.ui.language.code().into());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.sync_from_worker();
        self.show_dashboard(ui);
        if let Some(frames) = &mut self.smoke_frames {
            *frames = frames.saturating_sub(1);
            if *frames == 0 {
                eprintln!("ET54 GUI smoke test: three frames rendered; no device connected");
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ui.ctx().request_repaint();
            }
        }
        if self.state.connected || self.pending.is_some() {
            ui.ctx()
                .request_repaint_after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
