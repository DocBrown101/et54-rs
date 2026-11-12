//! Reusable widgets and plots; measurement values originate from worker snapshots.
use super::controls::BasicMode;
use crate::i18n::{Language, Text};
use crate::worker::Sample;
use eframe::egui;
use egui_plot::{AxisHints, Line, Plot, PlotPoints};
use std::collections::VecDeque;

pub fn configure_ui_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10., 10.);
        style.spacing.button_padding = egui::vec2(12., 7.);
        style.spacing.interact_size.y = 30.;
        style.spacing.combo_width = 140.;
        style
            .visuals
            .widgets
            .inactive
            .corner_radius = 6.into();
        style
            .visuals
            .widgets
            .hovered
            .corner_radius = 6.into();
        style
            .visuals
            .widgets
            .active
            .corner_radius = 6.into();
    });
}

pub(super) fn show_info_popup(ui: &mut egui::Ui, title: Text, contents: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let label = Language::get(ui.ctx()).text(title);
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui
            .small_button("ℹ")
            .on_hover_text(label);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        egui::Popup::from_toggle_button_response(&response)
            .id(ui.make_persistent_id(egui::IdSalt::new(title)))
            .accessible_name(label)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(contents);
        response
    })
    .inner
}

pub(super) fn show_measurements(ui: &mut egui::Ui, sample: Option<Sample>, stale: bool, mode: BasicMode) {
    let lang = Language::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.heading(lang.text(Text::Measurements));
        if stale {
            ui.label(lang.text(Text::Unavailable));
        }
    });
    let values = sample
        .map(|s| [Some(s.values.voltage), Some(s.values.current), Some(s.values.power), s.values.resistance])
        .unwrap_or([None; 4]);
    let columns = if ui.available_width() >= 660. { 4 } else { 2 };
    for row in (0..4).step_by(columns) {
        ui.columns(columns, |uis| {
            for (column, ui) in uis.iter_mut().enumerate() {
                let index = row + column;
                let (quantity_mode, label, unit) = [
                    (BasicMode::Cv, Text::Voltage, "V"),
                    (BasicMode::Cc, Text::Current, "A"),
                    (BasicMode::Cp, Text::Power, "W"),
                    (BasicMode::Cr, Text::Resistance, "Ω"),
                ][index];
                let width = ui.available_width();
                egui::Frame::group(ui.style())
                    .fill(ui.visuals().faint_bg_color)
                    .corner_radius(8)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_min_width((width - 26.).max(0.));
                        ui.vertical_centered(|ui| {
                            ui.weak(format!("{} [{unit}]", lang.text(label)));
                            let text = values[index]
                                .map(|v| format!("{v:.3}"))
                                .unwrap_or_else(|| "—".into());
                            let color = if stale { ui.visuals().weak_text_color() } else { ui.visuals().text_color() };
                            let mut value = egui::RichText::new(text)
                                .monospace()
                                .size(24.)
                                .color(color);
                            if !stale && mode == quantity_mode {
                                value = value
                                    .underline()
                                    .color(ui.visuals().selection.stroke.color);
                            }
                            ui.label(value);
                        });
                    });
            }
        });
    }
    if let Some(s) = sample {
        ui.weak(format!("{} {}", lang.text(Text::LastMeasurement), lang.age(s.when.elapsed().as_secs_f64())));
    }
}

pub fn show_voltage_current_plots(ui: &mut egui::Ui, history: &VecDeque<[f64; 3]>) {
    let lang = Language::get(ui.ctx());
    let axis_width_id = ui.make_persistent_id("curve_axis_width");
    let saved_width = ui.data(|data| data.get_temp::<f32>(axis_width_id));
    let axis_width = saved_width.unwrap_or(14.);
    let mut widest_axis = axis_width;
    let width = ui.available_width();
    let left = ui.available_rect_before_wrap().left();
    for (index, title, unit, color) in [
        (1, Text::Voltage, "V", egui::Color32::from_rgb(70, 150, 220)),
        (2, Text::Current, "A", egui::Color32::from_rgb(220, 140, 50)),
    ] {
        let points: PlotPoints<'_> = history
            .iter()
            .map(|p| [p[0], p[index]])
            .collect();
        let response = Plot::new(title)
            .width(width)
            .height(180.)
            .link_axis(egui::Id::unique("time"), [true, false])
            .x_axis_label(format!("{} [s]", lang.text(Text::Time)))
            .custom_y_axes(vec![
                AxisHints::new_y()
                    .label(unit)
                    .min_thickness(axis_width)
                    .label_spacing(20.0..=20.0),
            ])
            .label_formatter(move |hover| match hover {
                egui_plot::HoverPosition::NearDataPoint { position, .. } => Some(format!(
                    "{}: {:.3} {unit}\n{}: {:.2} s",
                    lang.text(title),
                    position.y,
                    lang.text(Text::Time),
                    position.x
                )),
                _ => None,
            })
            .show(ui, |plot| {
                if history.is_empty() {
                    plot.set_auto_bounds(true);
                }
                plot.line(
                    Line::new(lang.text(title), points)
                        .color(color)
                        .width(2.),
                );
            });
        widest_axis = widest_axis.max((response.response.rect.left() - left).ceil());
    }
    if saved_width != Some(widest_axis) {
        ui.data_mut(|data| data.insert_temp(axis_width_id, widest_axis));
        // Plot measures its labels for the next pass; share the widest margin without shrinking.
        ui.ctx()
            .request_discard("Shared plot axis width changed");
        ui.ctx().request_repaint();
    }
}

pub fn show_range_selector(ui: &mut egui::Ui, label: Text, value: &mut et54::Range) {
    let lang = Language::get(ui.ctx());
    egui::ComboBox::from_id_salt(label)
        .selected_text(format!("{}: {value}", lang.text(label)))
        .show_ui(ui, |ui| {
            for r in [et54::Range::High, et54::Range::Low] {
                ui.selectable_value(value, r, r.as_str());
            }
        });
}

pub fn show_quantity_input(ui: &mut egui::Ui, label: &str, value: &mut f64, unit: &str) {
    ui.label(label);
    ui.add_sized(
        [100., ui.spacing().interact_size.y],
        egui::DragValue::new(value)
            .speed(0.1)
            .suffix(format!(" {unit}"))
            .max_decimals(4),
    );
    ui.end_row();
}

#[cfg(test)]
#[path = "tests/widgets.rs"]
mod tests;
