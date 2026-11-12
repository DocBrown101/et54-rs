//! Measurement display and bounded history, keyed by connection epoch and channel.
use super::{Actions, Controls, controls::BasicMode, widgets};
use crate::i18n::{Language, Text};
use crate::worker::{Command, ControlState, Sample, Snapshot};
use eframe::egui;
use et54::Mode;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub(crate) struct Monitor {
    pub poll_s: f64,
    history: VecDeque<[f64; 3]>,
    origin: Instant,
    last_sample: Option<(u64, u64, u8)>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self {
            poll_s: 1.,
            history: VecDeque::new(),
            origin: Instant::now(),
            last_sample: None,
        }
    }
}
impl Monitor {
    pub fn reset_history(&mut self) {
        self.history.clear();
        self.last_sample = None;
        self.origin = Instant::now();
    }
    pub fn record_sample(&mut self, sample: Sample) {
        let key = (sample.epoch, sample.sequence, sample.channel);
        if self.last_sample != Some(key) {
            self.history.push_back([
                sample
                    .when
                    .saturating_duration_since(self.origin)
                    .as_secs_f64(),
                sample.values.voltage,
                sample.values.current,
            ]);
            if self.history.len() > 2000 {
                self.history.pop_front();
            }
            self.last_sample = Some(key);
        }
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, state: &Snapshot, controls: &Controls, read_s: f64, actions: &mut Actions<'_>) {
        let lang = Language::get(ui.ctx());
        if controls.battery_view {
            ui.heading(lang.text(Text::BatteryCounters));
            ui.label(lang.text(state.battery_state.label()));
            if let Some(sample) = state.battery_sample {
                let stale = !state.connected
                    || state.control != ControlState::Remote
                    || sample.channel != controls.selected
                    || sample.epoch != state.epoch
                    || state
                        .status
                        .is_none_or(|s| s.mode != Mode::Battery)
                    || sample.when.elapsed().as_secs_f64() > self.poll_s + 4. + read_s;
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{}: {:.4} Ah", lang.text(Text::Capacity), sample.capacity_ah));
                    ui.label(format!("{}: {:.4} Wh", lang.text(Text::Energy), sample.energy_wh));
                });
                ui.weak(format!(
                    "{} · {}{}",
                    lang.channel(sample.channel),
                    lang.age(sample.when.elapsed().as_secs_f64()),
                    if stale { format!(" · {}", lang.text(Text::Stale)) } else { String::new() }
                ));
            } else {
                ui.label(format!("{}: — Ah · {}: — Wh", lang.text(Text::Capacity), lang.text(Text::Energy)));
            }
            ui.small(lang.text(Text::CountersHelp));
            ui.separator();
        }
        if !state.connected && state.sample.is_none() {
            ui.label(lang.text(Text::ConnectForMeasurements));
            return;
        }
        let sample = state
            .sample
            .filter(|s| s.channel == controls.selected && s.epoch == state.epoch);
        let stale = !state.connected
            || state.control != ControlState::Remote
            || sample.is_none_or(|s| Duration::try_from_secs_f64(self.poll_s + 4. + read_s).map_or(true, |limit| s.when.elapsed() > limit));
        widgets::show_measurements(ui, sample, stale, if controls.battery_view { BasicMode::Cc } else { controls.basic_mode });
        egui::CollapsingHeader::new(lang.text(Text::MeasurementSettings))
            .id_salt("measurement_settings")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(lang.text(Text::PollPause));
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.poll_s)
                                .range(0.5..=60.)
                                .speed(0.1),
                        )
                        .changed()
                    {
                        match Duration::try_from_secs_f64(self.poll_s) {
                            Ok(interval) => actions.queue_command(Command::Interval(interval)),
                            Err(_) => *actions.error = Some(Text::InvalidPollPause.into()),
                        }
                    }
                });
                ui.small(lang.text(Text::PollHelp));
            });
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            ui.heading(lang.text(Text::LiveCurves));
            if ui
                .button(lang.text(Text::ClearCurves))
                .clicked()
            {
                self.history.clear();
                self.origin = Instant::now();
            }
        });
        ui.weak(lang.text(Text::HistoryHelp));
        widgets::show_voltage_current_plots(ui, &self.history);
    }
}

#[cfg(test)]
#[path = "tests/monitor.rs"]
mod tests;
