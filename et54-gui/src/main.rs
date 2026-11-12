mod app;
mod i18n;
mod ui;
mod worker;

fn main() -> eframe::Result {
    let smoke_test = std::env::args()
        .skip(1)
        .any(|arg| arg == "--smoke-test");
    eframe::run_native(
        "ET54",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1180., 860.])
                .with_min_inner_size([480., 600.]),
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(app::Et54App::new(cc, smoke_test)?))),
    )
}
