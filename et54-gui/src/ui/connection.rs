//! Connection drafts and port selection; connecting only emits a command.
use super::{Actions, widgets};
use crate::i18n::{Language, Message, Text};
use crate::worker::{Command, Snapshot};
use eframe::egui;
use et54::{ConnectionConfig, Model};
use std::time::Duration;

pub(crate) struct Connection {
    pub port: String,
    pub config: ConnectionConfig,
    pub read_s: f64,
    pub write_s: f64,
    pub delay_ms: u64,
    pub read_eol: usize,
    pub write_eol: usize,
}
impl Connection {
    pub fn new(port: String) -> Self {
        Self {
            port,
            config: ConnectionConfig::default(),
            read_s: 2.,
            write_s: 2.,
            delay_ms: 10,
            read_eol: 0,
            write_eol: 1,
        }
    }

    pub(super) fn show_details(&self, ui: &mut egui::Ui, state: &Snapshot) {
        let lang = Language::get(ui.ctx());
        widgets::show_info_popup(ui, Text::DeviceDetails, |ui| {
            if let Some(info) = &state.info {
                ui.label(format!(
                    "{} · SN {} · Firmware {} · Hardware {}",
                    info.model,
                    info.serial_number
                        .as_deref()
                        .unwrap_or(lang.text(Text::Unknown)),
                    info.firmware,
                    info.hardware
                ));
            }
            ui.label(format!("Port: {} · 9600 Baud", self.port));
            ui.label(format!(
                "{} / {}: {:.1} / {:.1} s · {}: {} · {}: {} · {}: {}",
                lang.text(Text::Read),
                lang.text(Text::Write),
                self.read_s,
                self.write_s,
                lang.text(Text::ResponsePause),
                self.delay_ms,
                lang.text(Text::Read),
                ["CRLF", "LF", "CR"][self.read_eol],
                lang.text(Text::Write),
                ["CRLF", "LF", "CR"][self.write_eol]
            ));
        });
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, state: &Snapshot, actions: &mut Actions<'_>) {
        let lang = Language::get(ui.ctx());
        ui.heading(lang.text(Text::ConnectDevice));
        ui.label(lang.text(Text::PortHelp));
        let waiting = actions.has_pending_command(state);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!waiting, egui::Button::new("⟳"))
                .on_hover_text(lang.text(Text::RefreshPorts))
                .clicked()
            {
                actions.queue_command(Command::Ports);
            }
            ui.add_enabled_ui(!state.connected && !waiting, |ui| {
                egui::ComboBox::from_id_salt("ports")
                    .selected_text(if self.port.is_empty() { lang.text(Text::SerialPorts) } else { &self.port })
                    .show_ui(ui, |ui| {
                        for p in &state.ports {
                            ui.selectable_value(&mut self.port, p.clone(), p);
                        }
                    });
            });
            if ui
                .add_enabled(!waiting && !self.port.trim().is_empty(), egui::Button::new(lang.text(Text::Connect)))
                .clicked()
            {
                let timeouts = Duration::try_from_secs_f64(self.read_s).and_then(|read| Duration::try_from_secs_f64(self.write_s).map(|write| (read, write)));
                let Ok((read, write)) = timeouts else {
                    *actions.error = Some(Text::InvalidTimeout.into());
                    return;
                };
                self.config.read_timeout = read;
                self.config.write_timeout = write;
                self.config.operation_delay = Duration::from_millis(self.delay_ms);
                let eols = [b"\r\n".to_vec(), b"\n".to_vec(), b"\r".to_vec()];
                self.config.read_termination = eols[self.read_eol].clone();
                self.config.write_termination = eols[self.write_eol].clone();
                match self.config.validate() {
                    Ok(()) => actions.queue_command(Command::Connect(self.port.trim().into(), self.config.clone())),
                    Err(e) => *actions.error = Some(Message::from(&e)),
                }
            }
        });
        egui::CollapsingHeader::new(lang.text(Text::ConnectionParameters))
            .id_salt("connection_parameters")
            .show(ui, |ui| {
                ui.add_enabled_ui(!state.connected && !waiting, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(lang.text(Text::ReadTimeout));
                        ui.add(
                            egui::DragValue::new(&mut self.read_s)
                                .range(0.1..=10.)
                                .speed(0.1),
                        );
                        ui.label(lang.text(Text::WriteTimeout));
                        ui.add(
                            egui::DragValue::new(&mut self.write_s)
                                .range(0.1..=10.)
                                .speed(0.1),
                        );
                        ui.label(lang.text(Text::ResponsePause));
                        ui.add(egui::DragValue::new(&mut self.delay_ms).range(0..=200))
                            .on_hover_text(lang.text(Text::NoResponsePause));
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (name, value) in [(Text::Read, &mut self.read_eol), (Text::Write, &mut self.write_eol)] {
                            egui::ComboBox::from_id_salt(name)
                                .selected_text(format!("{}: {}", lang.text(name), ["CRLF", "LF", "CR"][*value]))
                                .show_ui(ui, |ui| {
                                    for (i, s) in ["CRLF", "LF", "CR"].iter().enumerate() {
                                        ui.selectable_value(value, i, *s);
                                    }
                                });
                        }
                        egui::ComboBox::from_id_salt("override")
                            .selected_text(
                                self.config
                                    .model_override
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| lang.text(Text::AutoModel).into()),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.config.model_override, None, lang.text(Text::Automatic));
                                for m in Model::ALL {
                                    ui.selectable_value(&mut self.config.model_override, Some(m), m.to_string());
                                }
                            });
                    });
                });
            });
    }
}
