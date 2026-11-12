use super::*;

#[test]
fn info_popups_keep_titles_and_layout_and_close_on_escape_or_outside_click() {
    fn render(ctx: &egui::Context, title: Text, events: Vec<egui::Event>) -> (egui::Rect, egui::Rect, bool) {
        let mut result = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.))),
                events,
                ..Default::default()
            },
            |ui| {
                let mut visible = false;
                let icon = show_info_popup(ui, title, |ui| {
                    visible = true;
                    ui.label("Details");
                });
                result = Some((icon.rect, ui.label("Below the popup trigger").rect, visible));
            },
        );
        output.textures_delta.clear();
        let label = Language::get(ctx).text(title);
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label))
        );
        result.unwrap()
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    for language in [Language::German, Language::English] {
        for title in [Text::DeviceDetails, Text::SetpointReadback] {
            let ctx = egui::Context::default();
            configure_ui_style(&ctx);
            language.set(&ctx);
            let (icon, below, visible) = render(&ctx, title, vec![]);
            assert!(!visible);
            for escape in [true, false] {
                for pressed in [true, false] {
                    render(&ctx, title, pointer(icon.center(), pressed));
                }
                let (_, current_below, visible) = render(&ctx, title, vec![]);
                assert!(visible, "clicking the info icon must open the popup");
                assert_eq!(current_below, below, "opening the popup must not move the layout");
                if escape {
                    render(
                        &ctx,
                        title,
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                } else {
                    for pressed in [true, false] {
                        render(&ctx, title, pointer(egui::pos2(750., 550.), pressed));
                    }
                }
                let (_, current_below, visible) = render(&ctx, title, vec![]);
                assert!(!visible, "Escape or an outside click must close the popup");
                assert_eq!(current_below, below);
            }
        }
    }
}

#[test]
fn measurements_highlight_the_selected_quantity_only_when_current() {
    fn highlighted_values(shape: &egui::Shape, values: &mut Vec<String>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    highlighted_values(shape, values);
                }
            }
            egui::Shape::Text(text)
                if text
                    .galley
                    .job
                    .sections
                    .iter()
                    .any(|section| section.format.underline.width > 0.) =>
            {
                values.push(text.galley.job.text.clone());
            }
            _ => {}
        }
    }

    let ctx = egui::Context::default();
    let sample = Sample {
        values: et54::Measurements {
            voltage: 12.,
            current: 2.,
            power: 24.,
            resistance: Some(6.),
        },
        when: std::time::Instant::now(),
        sequence: 1,
        channel: 1,
        epoch: 1,
    };
    for (mode, expected) in [
        (BasicMode::Cc, "2.000"),
        (BasicMode::Cv, "12.000"),
        (BasicMode::Cp, "24.000"),
        (BasicMode::Cr, "6.000"),
    ] {
        for stale in [false, true] {
            let mut output = ctx.run_ui(Default::default(), |ui| show_measurements(ui, Some(sample), stale, mode));
            output.textures_delta.clear();
            let mut highlighted = Vec::new();
            for shape in output.shapes {
                highlighted_values(&shape.shape, &mut highlighted);
            }
            if stale {
                assert!(highlighted.is_empty());
            } else {
                assert_eq!(highlighted, [expected], "wrong highlighted quantity for {mode:?}");
            }
        }
    }
}

#[test]
fn clearing_curves_resets_zoom_and_resumes_auto_scaling() {
    let ctx = egui::Context::default();
    let mut history = VecDeque::from([[100., 12., 1.], [200., 13., 2.]]);
    for phase in 0..4 {
        let mut output = ctx.run_ui(Default::default(), |ui| {
            if phase == 1 {
                for title in [Text::Voltage, Text::Current] {
                    Plot::new(title)
                        .height(180.)
                        .link_axis(egui::Id::unique("time"), [true, false])
                        .show(ui, |plot| {
                            plot.set_plot_bounds(egui_plot::PlotBounds::from_min_max([150., 20.], [160., 30.]));
                        });
                }
            } else {
                show_voltage_current_plots(ui, &history);
            }
            for (index, title) in [(1, Text::Voltage), (2, Text::Current)] {
                let memory = egui_plot::PlotMemory::load(&ctx, ui.make_persistent_id(egui::IdSalt::new(title))).unwrap();
                assert_eq!(memory.auto_bounds.x, phase != 1);
                assert_eq!(memory.auto_bounds.y, phase != 1);
                if phase == 3 {
                    let bounds = memory.bounds();
                    for point in &history {
                        assert!(bounds.min()[0] <= point[0] && point[0] <= bounds.max()[0]);
                        assert!(bounds.min()[1] <= point[index] && point[index] <= bounds.max()[1]);
                    }
                }
            }
        });
        output.textures_delta.clear();
        if phase == 1 {
            history.clear();
        } else if phase == 2 {
            history = VecDeque::from([[0., 5., 0.1], [1., 6., 0.2]]);
        }
    }
}

#[test]
fn curves_align_plot_areas_and_keep_y_ticks_readable() {
    fn check_ticks(shape: &egui::Shape, plots: &[egui::Rect; 2], color: egui::Color32) -> usize {
        match shape {
            egui::Shape::Vec(shapes) => shapes
                .iter()
                .map(|shape| check_ticks(shape, plots, color))
                .sum(),
            egui::Shape::Text(text)
                if text
                    .galley
                    .job
                    .text
                    .chars()
                    .any(|c| c.is_ascii_digit())
                    && plots
                        .iter()
                        .any(|plot| text.pos.x < plot.left() && (plot.top()..=plot.bottom()).contains(&text.pos.y)) =>
            {
                for section in &text.galley.job.sections {
                    assert_eq!(section.format.color, color, "Y ticks must not fade");
                }
                1
            }
            _ => 0,
        }
    }

    let ctx = egui::Context::default();
    ctx.set_theme(egui::Theme::Dark);
    configure_ui_style(&ctx);
    let history = VecDeque::from([[0., 12.345678, 0.1], [1., 12.345679, 1.]]);
    let mut previous_margin = 0.;
    for (width, voltage_range) in [(1000., [12.345678, 12.345679]), (480., [1_234_567.123456, 1_234_567.123457]), (1200., [0., 5.])] {
        let mut plots = [egui::Rect::NOTHING; 2];
        let mut left = 0.;
        let mut ticks = 0;
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 600.))),
                    ..Default::default()
                },
                |ui| {
                    left = ui.available_rect_before_wrap().left();
                    let voltage_id = ui.make_persistent_id(egui::IdSalt::new(Text::Voltage));
                    if let Some(mut memory) = egui_plot::PlotMemory::load(&ctx, voltage_id) {
                        let bounds =
                            egui_plot::PlotBounds::from_min_max([memory.bounds().min()[0], voltage_range[0]], [memory.bounds().max()[0], voltage_range[1]]);
                        memory.auto_bounds.y = false;
                        memory.set_bounds(bounds);
                        memory.store(&ctx, voltage_id);
                    }
                    show_voltage_current_plots(ui, &history);
                    for (rect, title) in plots
                        .iter_mut()
                        .zip([Text::Voltage, Text::Current])
                    {
                        *rect = *egui_plot::PlotMemory::load(&ctx, ui.make_persistent_id(egui::IdSalt::new(title)))
                            .unwrap()
                            .transform()
                            .frame();
                    }
                },
            );
            output.textures_delta.clear();
            ticks = output
                .shapes
                .iter()
                .map(|shape| {
                    check_ticks(
                        &shape.shape,
                        &plots,
                        ctx.style_of(egui::Theme::Dark)
                            .visuals
                            .text_color(),
                    )
                })
                .sum::<usize>();
        }
        assert!((plots[0].left() - plots[1].left()).abs() < 0.01);
        assert!((plots[0].right() - plots[1].right()).abs() < 0.01);
        assert!(
            plots
                .iter()
                .all(|plot| plot.width() > 0.)
        );
        assert!(ticks > 0, "the test must exercise visible Y ticks");
        let margin = plots[0].left() - left;
        assert!(margin >= previous_margin, "the shared margin must not shrink");
        previous_margin = margin;
    }
}
