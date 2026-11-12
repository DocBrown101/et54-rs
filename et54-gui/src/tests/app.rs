use super::*;
use std::time::Instant;

#[derive(Default)]
struct MemoryStorage(std::collections::HashMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.into(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn last_successful_port_is_restored_instead_of_an_unconnected_selection() {
    use eframe::App;

    let ctx = egui::Context::default();
    let mut storage = MemoryStorage::default();
    let mut app = Et54App::with_worker(Worker::start(ctx.clone()).unwrap(), None);
    assert!(app.ui.connection.port.is_empty());
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.pending.is_some() {
        assert!(Instant::now() < deadline, "port discovery timeout");
        app.sync_from_worker();
        std::thread::sleep(Duration::from_millis(2));
    }
    app.worker
        .state
        .lock()
        .unwrap()
        .last_connected_port = Some("port-a".into());
    app.ui.connection.port = "port-b".into();
    app.save(&mut storage);
    drop(app);

    let mut app = Et54App::with_worker(Worker::start(ctx.clone()).unwrap(), Some(&storage));
    assert_eq!(app.ui.connection.port, "port-a");
    assert!(!app.state.connected);
    app.ui.connection.port = "port-b".into();
    app.save(&mut storage);
    drop(app);

    let app = Et54App::with_worker(Worker::start(ctx).unwrap(), Some(&storage));
    assert_eq!(app.ui.connection.port, "port-a");
}

#[test]
fn language_preference_is_saved_and_restored_without_connecting() {
    use eframe::App;
    let ctx = egui::Context::default();
    let mut storage = MemoryStorage::default();
    for language in [Language::German, Language::English] {
        let mut app = Et54App::with_worker(Worker::start(ctx.clone()).unwrap(), None);
        assert_eq!(app.ui.language, Language::English);
        app.ui.language = language;
        app.save(&mut storage);
        drop(app);
        let app = Et54App::with_worker(Worker::start(ctx.clone()).unwrap(), Some(&storage));
        assert_eq!(app.ui.language, language);
        assert!(!app.state.connected);
    }
}

#[test]
fn theme_preference_survives_egui_memory_persistence_and_style_setup() {
    let ctx = egui::Context::default();
    let mut storage = MemoryStorage::default();
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.set_theme(theme);
        ctx.memory(|memory| eframe::set_value(&mut storage, "egui", memory));
        let restored = egui::Context::default();
        restored.memory_mut(|memory| *memory = eframe::get_value(&storage, "egui").unwrap());
        ui::configure_ui_style(&restored);
        assert_eq!(restored.theme(), theme);
        assert_eq!(restored.options(|options| options.theme_preference), theme.into());
    }
}
