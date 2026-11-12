use super::*;

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

fn app_with_mock_ports(storage: Option<&dyn eframe::Storage>) -> Et54App {
    let (worker, commands) = Worker::test_channel();
    let mut app = Et54App::with_worker(worker, storage);
    let (id, command) = commands.try_recv().unwrap();
    assert!(matches!(command, Command::Ports));
    {
        let mut state = app.worker.state.lock().unwrap();
        state.ports = vec!["port-a".into(), "port-b".into()];
        state.completed = id;
    }
    app.sync_from_worker();
    assert!(app.pending.is_none());
    app
}

#[test]
fn last_successful_port_is_restored_instead_of_an_unconnected_selection() {
    use eframe::App;

    let mut storage = MemoryStorage::default();
    let mut app = app_with_mock_ports(None);
    assert!(app.ui.connection.port.is_empty());
    assert_eq!(app.state.ports, ["port-a", "port-b"]);
    app.worker
        .state
        .lock()
        .unwrap()
        .last_connected_port = Some("port-a".into());
    app.ui.connection.port = "port-b".into();
    app.save(&mut storage);
    drop(app);

    let mut app = app_with_mock_ports(Some(&storage));
    assert_eq!(app.ui.connection.port, "port-a");
    assert!(!app.state.connected);
    app.ui.connection.port = "port-b".into();
    app.save(&mut storage);
    drop(app);

    let app = app_with_mock_ports(Some(&storage));
    assert_eq!(app.ui.connection.port, "port-a");
}

#[test]
fn language_preference_is_saved_and_restored_without_connecting() {
    use eframe::App;
    let mut storage = MemoryStorage::default();
    for language in [Language::German, Language::English] {
        let mut app = app_with_mock_ports(None);
        assert_eq!(app.ui.language, Language::English);
        app.ui.language = language;
        app.save(&mut storage);
        drop(app);
        let app = app_with_mock_ports(Some(&storage));
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
