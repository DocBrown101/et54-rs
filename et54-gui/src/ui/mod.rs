//! Dashboard layout, snapshot synchronization and UI actions. No device I/O.
mod connection;
mod controls;
mod monitor;
mod status;
mod widgets;

pub(crate) use connection::Connection;
use controls::Controls;
use monitor::Monitor;
pub(crate) use widgets::configure_ui_style;

use crate::i18n::{Language, Message, Text};
use crate::worker::{Command, Snapshot};
use eframe::egui;

pub(crate) struct Dashboard {
    pub language: Language,
    pub connection: Connection,
    controls: Controls,
    monitor: Monitor,
    history_epoch: u64,
}

/// Pending actions disable subsequent controls in this frame before the app sends them.
struct Actions<'a> {
    commands: Vec<Command>,
    pending: bool,
    error: &'a mut Option<Message>,
}
impl Actions<'_> {
    fn queue_command(&mut self, command: Command) {
        self.commands.push(command);
        self.pending = true;
        *self.error = None;
    }
    fn has_pending_command(&self, state: &Snapshot) -> bool {
        self.pending || state.busy
    }
}

impl Dashboard {
    pub fn new(port: String) -> Self {
        Self {
            language: Language::default(),
            connection: Connection::new(port),
            controls: Controls::default(),
            monitor: Monitor::default(),
            history_epoch: 0,
        }
    }

    pub fn sync_from_snapshot(&mut self, state: &Snapshot, pending: bool) {
        if !pending && !state.busy && self.controls.selected != state.selected {
            self.controls.selected = state.selected;
            self.monitor.reset_history();
        }
        if self.history_epoch != state.epoch {
            self.monitor.reset_history();
            self.history_epoch = state.epoch;
            self.controls.selected = state.selected;
        }
        self.controls.sync_from_snapshot(state);
        if let Some(sample) = state
            .sample
            .filter(|s| s.channel == self.controls.selected && s.epoch == state.epoch)
        {
            self.monitor.record_sample(sample);
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &Snapshot, pending: bool, error: &mut Option<Message>) -> Vec<Command> {
        self.language.set(ui.ctx());
        let mut actions = Actions {
            commands: Vec::new(),
            pending,
            error,
        };
        egui::Panel::top("header").show(ui, |ui| {
            status::show_status_header(ui, state, &self.connection, self.controls.selected, &mut self.language, &mut actions)
        });
        self.language.set(ui.ctx());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(self.language.text(Text::Title).into()));
        egui::Panel::bottom("notifications").show(ui, |ui| status::show_notifications(ui, state, &actions));
        if !state.connected {
            egui::Panel::top("connection").show(ui, |ui| {
                self.connection
                    .show(ui, state, &mut actions)
            });
        }
        let wide = ui.available_width() >= 900.;
        if state.connected && wide {
            egui::Panel::left("controls")
                .exact_size(340.)
                .resizable(false)
                .frame(egui::Frame::central_panel(ui.style()))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("controls_scroll")
                        .show(ui, |ui| self.show_controls(ui, state, &mut actions));
                });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("monitor_scroll")
                .show(ui, |ui| {
                    self.monitor
                        .show(ui, state, &self.controls, self.connection.read_s, &mut actions);
                    if state.connected && !wide {
                        ui.separator();
                        self.show_controls(ui, state, &mut actions);
                    }
                });
        });
        actions.commands
    }

    fn show_controls(&mut self, ui: &mut egui::Ui, state: &Snapshot, actions: &mut Actions<'_>) {
        let selected = self.controls.selected;
        self.controls.show(ui, state, actions);
        if self.controls.selected != selected {
            self.monitor.reset_history();
        }
    }
}

#[cfg(test)]
mod tests;
