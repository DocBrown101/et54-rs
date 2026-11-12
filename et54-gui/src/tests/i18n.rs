use super::*;

#[test]
fn both_languages_cover_every_key_and_unknown_preferences_use_english() {
    for key in ALL_TEXTS {
        for language in [Language::English, Language::German] {
            assert!(!language.text(*key).trim().is_empty(), "missing {language:?}/{key:?}");
            assert_eq!(Language::from_code(language.code()), language);
        }
    }
    assert_eq!(Language::from_code("unknown"), Language::English);
    assert_eq!(Language::default(), Language::English);
}

#[test]
fn notifications_translate_at_display_time_without_changing_device_details() {
    let message = Message::ChannelRead(2);
    assert_eq!(message.render(Language::English), "Channel 2: Device settings read");
    assert_eq!(message.render(Language::German), "Kanal 2: Geräteeinstellungen gelesen");
    let error = et54::Error::DisableFailed {
        channel: 2,
        source: Box::new(et54::Error::Device {
            command: "Ch2:SW OFF".into(),
            response: "Rexecu err".into(),
        }),
    };
    let message = Message::from(&error);
    for (language, warning) in [(Language::German, "EINGANGSZUSTAND UNBEKANNT"), (Language::English, "INPUT STATE UNKNOWN")] {
        let rendered = message.render(language);
        assert!(rendered.contains(warning));
        assert!(rendered.contains("Ch2:SW OFF") && rendered.contains("Rexecu err"));
    }
    let invalid = Message::from(&et54::Error::Invalid(Text::PositiveCurrent.english().into()));
    assert_eq!(invalid.render(Language::German), "Entladestrom muss größer als 0 A sein");
    assert_eq!(invalid.render(Language::English), "discharge current must be greater than 0 A");
}
