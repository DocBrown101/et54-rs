//! Persistent status, notifications and OFF, including while another command is pending.
use super::{Actions, Connection};
use crate::i18n::{Language, Text};
use crate::worker::{Command, ControlState, Snapshot};
use eframe::egui;
use et54::{InputState, Protection};

pub(super) fn show_status_header(
    ui: &mut egui::Ui,
    state: &Snapshot,
    connection: &Connection,
    selected: u8,
    language: &mut Language,
    actions: &mut Actions<'_>,
) {
    let lang = *language;
    let waiting = actions.has_pending_command(state);
    ui.horizontal_wrapped(|ui| {
        ui.heading(
            state
                .info
                .as_ref()
                .map_or_else(|| "ET54".into(), |info| info.model.to_string()),
        );
        ui.label(lang.text(if state.connected { Text::Connected } else { Text::Disconnected }));
        if let Some(status) = state
            .status
            .filter(|_| state.connected && state.selected == selected)
        {
            let (text, color) = match status.input {
                InputState::On => (Text::InputOn, ui.visuals().warn_fg_color),
                InputState::Off => (Text::InputOff, ui.visuals().text_color()),
            };
            ui.colored_label(color, lang.text(text));
            ui.label(format!("{} {}", lang.text(Text::Mode), status.mode));
            ui.colored_label(
                if status.protection == Protection::None {
                    ui.visuals().weak_text_color()
                } else {
                    ui.visuals().error_fg_color
                },
                format!("{} {}", lang.text(Text::Protection), status.protection),
            );
        } else {
            ui.colored_label(ui.visuals().warn_fg_color, lang.text(Text::InputUnknown));
        }
        if state.connected {
            let (color, label) = match state.control {
                ControlState::Remote => (ui.visuals().text_color(), Text::RemoteControl),
                ControlState::Local => (ui.visuals().warn_fg_color, Text::LocalControl),
                ControlState::Unknown => (ui.visuals().warn_fg_color, Text::ControlUnknown),
            };
            ui.colored_label(color, lang.text(label))
                .on_hover_text(lang.text(Text::LockHelp));
            let remote = state.control == ControlState::Remote;
            if ui
                .add_enabled(
                    !waiting,
                    egui::Button::new(lang.text(if remote { Text::ReleaseLocal } else { Text::ResumeRemote })),
                )
                .clicked()
            {
                actions.queue_command(if remote { Command::Local } else { Command::Remote });
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if state.connected {
            // Keep OFF outside scrolling controls, including while configuration is running.
            if ui
                .add(egui::Button::new(
                    egui::RichText::new(lang.text(Text::SwitchOff))
                        .strong()
                        .color(ui.visuals().error_fg_color),
                ))
                .clicked()
            {
                actions.queue_command(Command::Disable);
            }
            if ui
                .add_enabled(!waiting, egui::Button::new(lang.text(Text::Disconnect)))
                .clicked()
            {
                actions.queue_command(Command::Disconnect);
            }
            connection.show_details(ui, state);
        }
        if ui.available_size_before_wrap().x < 220. {
            ui.end_row();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dark = ui.ctx().theme() == egui::Theme::Dark;
            if ui
                .button(if dark { "☀" } else { "🌙" })
                .on_hover_text(lang.text(if dark { Text::SwitchToLight } else { Text::SwitchToDark }))
                .clicked()
            {
                ui.ctx()
                    .set_theme(if dark { egui::Theme::Light } else { egui::Theme::Dark });
            }
            ui.selectable_value(language, Language::German, "Deutsch");
            ui.selectable_value(language, Language::English, "English");
        });
    });
}

pub(super) fn show_notifications(ui: &mut egui::Ui, state: &Snapshot, actions: &Actions<'_>) {
    let lang = Language::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if actions.has_pending_command(state) {
            ui.spinner();
            ui.label(lang.text(Text::CommandRunning));
        }
        ui.label(state.message.render(lang));
    });
    if state.control == ControlState::Local {
        ui.small(lang.text(Text::LocalHelp));
    }
    if let Some(error) = actions
        .error
        .as_ref()
        .or(state.error.as_ref())
    {
        ui.colored_label(ui.visuals().error_fg_color, error.render(lang));
    }
}
