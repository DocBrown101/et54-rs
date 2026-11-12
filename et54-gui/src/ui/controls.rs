//! Editable drafts stay separate from device readbacks; only explicit actions emit commands.
use super::{Actions, widgets};
use crate::i18n::{Language, Message, Text};
use crate::worker::{BatteryReadback, BatterySettings, Command, ControlState, Snapshot, validate_gui_settings};
use eframe::egui;
use et54::{BatteryCutoff, BatterySubmode, InputState, LoadMode, Mode, Model, ProtectionLimits, Range, Result, VoltageThresholds};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BasicMode {
    Cc,
    Cv,
    Cp,
    Cr,
}
impl BasicMode {
    fn setpoint_index(self) -> usize {
        match self {
            Self::Cc => 0,
            Self::Cv => 1,
            Self::Cp => 2,
            Self::Cr => 3,
        }
    }
    fn setpoint_label_and_unit(self) -> (Text, &'static str) {
        match self {
            Self::Cc => (Text::CurrentSetpoint, "A"),
            Self::Cv => (Text::VoltageSetpoint, "V"),
            Self::Cp => (Text::PowerSetpoint, "W"),
            Self::Cr => (Text::ResistanceSetpoint, "Ω"),
        }
    }
}

pub(crate) struct Controls {
    pub(super) basic_mode: BasicMode,
    pub(super) battery_draft: BatterySettings,
    pub(super) battery_view: bool,
    pub(super) protections: ProtectionLimits,
    pub(super) ranges: [Range; 2],
    pub(super) selected: u8,
    pub(super) setpoints: [f64; 4],
    pub(super) voltage_thresholds: VoltageThresholds,
    settings_channel: Option<(u64, u8)>,
    settings_revision: u64,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            selected: 1,
            basic_mode: BasicMode::Cc,
            battery_view: false,
            battery_draft: BatterySettings { current: 0., cutoff: 0.1 },
            setpoints: [0.; 4],
            settings_channel: None,
            settings_revision: 0,
            protections: ProtectionLimits {
                voltage: 0.,
                current: 0.,
                power: 0.,
            },
            ranges: [Range::High; 2],
            voltage_thresholds: VoltageThresholds { on: 0.1, off: 0.1 },
        }
    }
}
impl Controls {
    pub(super) fn sync_from_snapshot(&mut self, state: &Snapshot) {
        if self.settings_revision == state.settings_revision || state.selected != self.selected {
            return;
        }
        self.settings_revision = state.settings_revision;
        let channel = (state.epoch, state.selected);
        if self.settings_channel != Some(channel) {
            self.battery_view = state
                .status
                .is_some_and(|s| s.mode == Mode::Battery);
            self.battery_draft = BatterySettings { current: 0., cutoff: 0.1 };
            self.basic_mode = match state.settings {
                Some(LoadMode::ConstantVoltage(_)) => BasicMode::Cv,
                Some(LoadMode::ConstantPower(_)) => BasicMode::Cp,
                Some(LoadMode::ConstantResistance(_)) => BasicMode::Cr,
                _ => BasicMode::Cc,
            };
            self.settings_channel = state.basic_setpoints.map(|_| channel);
        }
        self.setpoints = state.basic_setpoints.unwrap_or([0.; 4]);
        if let Some(battery) = state.battery {
            // Show the first stage as an editable proposal, but never flatten a readback.
            if battery.config.submode == BatterySubmode::Cc && battery.config.cutoff == BatteryCutoff::Voltage {
                self.battery_draft = BatterySettings {
                    current: battery.config.value.stages()[0],
                    cutoff: battery.config.cutoff_value.stages()[0],
                };
            }
        }
        if let Some(p) = state.protections {
            self.protections = p;
        }
        if let Some(r) = state.ranges {
            self.ranges = r;
        }
        if let Some(thresholds) = state.voltage_thresholds {
            self.voltage_thresholds = thresholds;
        }
    }

    pub fn draft_load_mode(&self) -> LoadMode {
        let value = self.setpoints[self.basic_mode.setpoint_index()];
        match self.basic_mode {
            BasicMode::Cc => LoadMode::ConstantCurrent(value),
            BasicMode::Cv => LoadMode::ConstantVoltage(value),
            BasicMode::Cp => LoadMode::ConstantPower(value),
            BasicMode::Cr => LoadMode::ConstantResistance(value),
        }
    }
    pub fn settings_changed(&self, state: &Snapshot) -> bool {
        (if self.battery_view {
            state
                .battery
                .and_then(BatteryReadback::single_stage_cc_settings)
                != Some(self.battery_draft)
                || state
                    .status
                    .is_none_or(|s| s.mode != Mode::Battery)
        } else {
            state.settings != Some(self.draft_load_mode())
        }) || state.protections != Some(self.protections)
            || state.ranges != Some(self.ranges)
            || state.voltage_thresholds != Some(self.voltage_thresholds)
    }
    fn show_common_settings(&mut self, ui: &mut egui::Ui) {
        let lang = Language::get(ui.ctx());
        ui.label(egui::RichText::new(lang.text(Text::Ranges)).strong());
        widgets::show_range_selector(ui, Text::VoltageRange, &mut self.ranges[0]);
        widgets::show_range_selector(ui, Text::CurrentRange, &mut self.ranges[1]);
        ui.label(egui::RichText::new(lang.text(Text::ProtectionThresholds)).strong());
        egui::Grid::new("protections")
            .num_columns(2)
            .show(ui, |ui| {
                widgets::show_quantity_input(ui, "OVP", &mut self.protections.voltage, "V");
                widgets::show_quantity_input(ui, "OCP", &mut self.protections.current, "A");
                widgets::show_quantity_input(ui, "OPP", &mut self.protections.power, "W");
                widgets::show_quantity_input(ui, lang.text(Text::VoltageOn), &mut self.voltage_thresholds.on, "V");
                widgets::show_quantity_input(ui, lang.text(Text::VoltageOff), &mut self.voltage_thresholds.off, "V");
            });
    }
    fn show_channel_selection(&mut self, ui: &mut egui::Ui, state: &Snapshot, ready: bool, actions: &mut Actions<'_>) {
        let lang = Language::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(ready, |ui| {
                let old = self.selected;
                let channels = state
                    .info
                    .as_ref()
                    .map_or(1, |i| i.model.channel_count());
                if channels > 1 {
                    egui::ComboBox::from_id_salt("channel")
                        .selected_text(lang.channel(self.selected))
                        .show_ui(ui, |ui| {
                            for n in 1..=channels {
                                ui.selectable_value(&mut self.selected, n, lang.channel(n));
                            }
                        });
                }
                if old != self.selected {
                    actions.queue_command(Command::Select(self.selected));
                }
                if ui
                    .button(lang.text(Text::ReadDevice))
                    .on_hover_text(lang.text(Text::ReadAgain))
                    .clicked()
                {
                    actions.queue_command(Command::Refresh);
                }
            });
        });
    }

    fn show_readback(&self, ui: &mut egui::Ui, state: &Snapshot) {
        let lang = Language::get(ui.ctx());
        if let Some(settings) = state
            .settings
            .filter(|_| state.selected == self.selected)
        {
            widgets::show_info_popup(ui, Text::SetpointReadback, |ui| {
                ui.label(format!("{settings:?}"));
            });
        }
        if let Some(battery) = state
            .battery
            .filter(|_| self.battery_view && state.selected == self.selected)
        {
            egui::CollapsingHeader::new(lang.text(Text::BatteryReadback))
                .id_salt("battery_readback")
                .show(ui, |ui| {
                    ui.label(format!("{:?} · {} {}", battery.config, battery.stages, lang.text(Text::Stages)));
                });
            if battery
                .single_stage_cc_settings()
                .is_none()
            {
                ui.colored_label(ui.visuals().warn_fg_color, lang.text(Text::OtherBatteryProfile));
            }
        }
        if state.basic_setpoints.is_none() {
            ui.weak(lang.text(Text::SetpointsUnavailable));
        }
    }

    fn show_basic_settings(&mut self, ui: &mut egui::Ui) {
        let lang = Language::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            for (mode, label) in [(BasicMode::Cv, "CV"), (BasicMode::Cc, "CC"), (BasicMode::Cp, "CP"), (BasicMode::Cr, "CR")] {
                ui.selectable_value(&mut self.basic_mode, mode, label);
            }
        });
        egui::Grid::new("setpoints")
            .num_columns(2)
            .show(ui, |ui| {
                let (label, unit) = self
                    .basic_mode
                    .setpoint_label_and_unit();
                widgets::show_quantity_input(ui, lang.text(label), &mut self.setpoints[self.basic_mode.setpoint_index()], unit);
            });
    }

    fn show_battery_settings(&mut self, ui: &mut egui::Ui) {
        let lang = Language::get(ui.ctx());
        ui.label(lang.text(Text::SingleStage));
        egui::Grid::new("battery_setpoints")
            .num_columns(2)
            .show(ui, |ui| {
                widgets::show_quantity_input(ui, lang.text(Text::DischargeCurrent), &mut self.battery_draft.current, "A");
                widgets::show_quantity_input(ui, lang.text(Text::BatteryCutoff), &mut self.battery_draft.cutoff, "V");
            });
        if self.voltage_thresholds.off > self.battery_draft.cutoff {
            ui.small(lang.cutoff_warning(self.voltage_thresholds.off, self.battery_draft.cutoff));
        }
    }

    fn validate_draft(&self, model: Model) -> Result<()> {
        if self.battery_view {
            self.battery_draft
                .validate(model, self.protections, self.ranges, self.voltage_thresholds)
        } else {
            validate_gui_settings(model, self.draft_load_mode(), self.protections, self.ranges, self.voltage_thresholds)
        }
    }

    fn configure_command(&self) -> Command {
        if self.battery_view {
            Command::ConfigureBattery {
                battery: self.battery_draft,
                protections: self.protections,
                ranges: self.ranges,
                voltage_thresholds: self.voltage_thresholds,
            }
        } else {
            Command::Configure {
                mode: self.draft_load_mode(),
                protections: self.protections,
                ranges: self.ranges,
                voltage_thresholds: self.voltage_thresholds,
            }
        }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, state: &Snapshot, actions: &mut Actions<'_>) {
        let lang = Language::get(ui.ctx());
        ui.heading(lang.text(Text::Controls));
        let ready = state.connected && state.control == ControlState::Remote && !actions.has_pending_command(state) && state.selected == self.selected;
        self.show_channel_selection(ui, state, ready, actions);
        self.show_readback(ui, state);
        let editable = ready && !actions.pending && state.selected == self.selected && state.basic_setpoints.is_some();
        let settings_ui = ui.add_enabled_ui(editable, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.battery_view, false, lang.text(Text::BasicOperation));
                ui.selectable_value(&mut self.battery_view, true, lang.text(Text::Battery));
            });
            if self.battery_view {
                self.show_battery_settings(ui);
            } else {
                self.show_basic_settings(ui);
            }
            ui.separator();
            if self.battery_view {
                egui::CollapsingHeader::new(lang.text(Text::CommonSettings))
                    .id_salt("common_settings")
                    .show(ui, |ui| self.show_common_settings(ui));
            } else {
                self.show_common_settings(ui);
            }
            let validation = state
                .info
                .as_ref()
                .map(|info| self.validate_draft(info.model));
            let valid = matches!(validation, Some(Ok(())));
            if let Some(Err(error)) = validation {
                ui.colored_label(ui.visuals().error_fg_color, Message::from(&error).render(lang));
            }
            let dirty = self.settings_changed(state);
            if dirty {
                ui.colored_label(ui.visuals().warn_fg_color, lang.text(Text::NotApplied));
                ui.small(lang.text(Text::ApplyFirst));
            }
            if ui
                .add_enabled(valid, egui::Button::new(lang.text(Text::Apply)))
                .clicked()
            {
                actions.queue_command(self.configure_command());
            }
            valid
                && !dirty
                && state.has_current_settings()
                && (!self.battery_view
                    || state
                        .status
                        .is_some_and(|s| s.input == InputState::Off))
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    editable && settings_ui.inner,
                    egui::Button::new(if self.battery_view {
                        egui::RichText::new(lang.text(Text::StartDischarge))
                    } else {
                        egui::RichText::new(lang.text(Text::InputOn)).color(egui::Color32::from_rgb(0, 140, 70))
                    }),
                )
                .on_hover_text(lang.text(Text::CheckBeforeOn))
                .clicked()
            {
                actions.queue_command(Command::Enable);
            }
        });
    }
}

#[cfg(test)]
#[path = "tests/controls.rs"]
mod tests;
