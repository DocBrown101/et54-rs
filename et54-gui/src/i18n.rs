//! UI text and deferred notifications. Device responses remain verbatim.
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    German,
    #[default]
    English,
}

impl Language {
    pub fn from_code(code: &str) -> Self {
        if code == "de" { Self::German } else { Self::English }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::German => "de",
            Self::English => "en",
        }
    }
    pub fn get(ctx: &egui::Context) -> Self {
        ctx.data(|data| {
            data.get_temp(egui::Id::unique("ui_language"))
                .unwrap_or_default()
        })
    }
    pub fn set(self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique("ui_language"), self));
    }
    pub fn channel(self, number: u8) -> String {
        format!("{} {number}", self.text(Text::Channel))
    }
    pub fn age(self, seconds: f64) -> String {
        match self {
            Self::German => format!("vor {seconds:.1} s"),
            Self::English => format!("{seconds:.1} s ago"),
        }
    }
    pub fn cutoff_warning(self, off: f64, cutoff: f64) -> String {
        match self {
            Self::German => format!(
                "V-off ({off:.3} V) liegt über der Batterie-Abschaltspannung ({cutoff:.3} V). Der Eingang kann dadurch früher abschalten. V-off ist unter ‚Bereiche, Schutzgrenzen & V-on/V-off‘ einstellbar."
            ),
            Self::English => format!(
                "V-off ({off:.3} V) exceeds the battery cutoff voltage ({cutoff:.3} V). The input may switch off earlier. Adjust V-off under ‘Ranges, protection & V-on/V-off’."
            ),
        }
    }
}

macro_rules! texts {
    ($($key:ident => ($de:literal, $en:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Text { $($key),+ }
        impl Language {
            pub fn text(self, key: Text) -> &'static str {
                match (self, key) {
                    $((Self::German, Text::$key) => $de,
                      (Self::English, Text::$key) => $en),+
                }
            }
        }
        #[cfg(test)]
        const ALL_TEXTS: &[Text] = &[$(Text::$key),+];
    };
}

texts! {
    Title => ("ET54 – Elektronische Last", "ET54 – Electronic load"),
    SwitchToLight => ("Zum hellen Design wechseln", "Switch to light mode"),
    SwitchToDark => ("Zum dunklen Design wechseln", "Switch to dark mode"),
    DeviceDetails => ("Geräte- und Verbindungsdetails", "Device and connection details"),
    Unknown => ("unbekannt", "unknown"),
    Read => ("Lesen", "Read"),
    Write => ("Schreiben", "Write"),
    ResponsePause => ("Pause nach Antwort [ms]", "Response pause [ms]"),
    ConnectDevice => ("Gerät verbinden", "Connect device"),
    PortHelp => ("Seriellen Port auswählen oder den Gerätepfad eingeben · 9600 Baud", "Select a serial port or enter the device path · 9600 baud"),
    RefreshPorts => ("Ports neu einlesen", "Refresh ports"),
    SerialPorts => ("Serielle Ports", "Serial ports"),
    Connect => ("Verbinden", "Connect"),
    ConnectionParameters => ("Verbindungsparameter / Mustool-Modell", "Connection settings / Mustool model"),
    ReadTimeout => ("Read-Timeout [s]", "Read timeout [s]"),
    WriteTimeout => ("Write-Timeout [s]", "Write timeout [s]"),
    NoResponsePause => ("0 ms: keine zusätzliche Pause nach der Geräteantwort.", "0 ms: no additional pause after the device response."),
    AutoModel => ("Modell automatisch", "Automatic model detection"),
    Automatic => ("Automatisch", "Automatic"),
    CurrentSetpoint => ("Sollstrom", "Current setpoint"),
    VoltageSetpoint => ("Sollspannung", "Voltage setpoint"),
    PowerSetpoint => ("Sollleistung", "Power setpoint"),
    ResistanceSetpoint => ("Sollwiderstand", "Resistance setpoint"),
    Ranges => ("Bereiche", "Ranges"),
    VoltageRange => ("Spannungsbereich", "Voltage range"),
    CurrentRange => ("Strombereich", "Current range"),
    ProtectionThresholds => ("Schutzgrenzen & Schaltschwellen", "Protection limits & switching thresholds"),
    VoltageOn => ("V-on (Startspannung)", "V-on (start voltage)"),
    VoltageOff => ("V-off (Abschaltspannung)", "V-off (cutoff voltage)"),
    Channel => ("Kanal", "Channel"),
    ReadDevice => ("Vom Gerät lesen", "Read from device"),
    ReadAgain => ("Geräteeinstellungen erneut lesen", "Read device settings again"),
    SetpointReadback => ("Zurückgelesene Sollwerte", "Setpoint readback"),
    BatteryReadback => ("Zurückgelesene Batterieeinstellungen", "Battery settings readback"),
    Stages => ("Stufe(n)", "stage(s)"),
    OtherBatteryProfile => ("Andere Batteriekonfiguration aktiv. Diese Ansicht startet nur einstufiges CC mit Spannungsabschaltung. Anwenden ersetzt das vorhandene Profil.", "Another battery configuration is active. This view only starts single-stage CC with voltage cutoff. Applying replaces the existing profile."),
    SetpointsUnavailable => ("Sollwerte nicht verfügbar – zuerst vom Gerät lesen.", "Setpoints unavailable – read from the device first."),
    SingleStage => ("Einstufige CC-Entladung · Abschaltung im Gerät", "Single-stage CC discharge · cutoff controlled by the device"),
    DischargeCurrent => ("Entladestrom", "Discharge current"),
    BatteryCutoff => ("Batterie-Abschaltspannung", "Battery cutoff voltage"),
    Controls => ("Steuerung", "Controls"),
    BasicOperation => ("Basisbetrieb", "Basic operation"),
    Battery => ("Batterie", "Battery"),
    CommonSettings => ("Bereiche, Schutzgrenzen & V-on/V-off", "Ranges, protection & V-on/V-off"),
    NotApplied => ("Nicht angewendet", "Not applied"),
    ApplyFirst => ("EIN verwendet ausschließlich zurückgelesene Geräteeinstellungen. Geänderte Sollwerte zuerst anwenden.", "ON uses only settings read back from the device. Apply changed setpoints first."),
    Apply => ("Anwenden (zuerst AUS)", "Apply (switch OFF first)"),
    StartDischarge => ("Entladung starten", "Start discharge"),
    InputOn => ("Eingang EIN", "Input ON"),
    InputOff => ("Eingang AUS", "Input OFF"),
    CheckBeforeOn => ("Vor dem Einschalten werden die tatsächlichen Geräteeinstellungen erneut geprüft.", "Actual device settings are checked again before switching ON."),
    Connected => ("Verbunden", "Connected"),
    Disconnected => ("Nicht verbunden", "Not connected"),
    Mode => ("Modus", "Mode"),
    Protection => ("Schutz", "Protection"),
    InputUnknown => ("Eingangszustand unbekannt", "Input state unknown"),
    SwitchOff => ("EINGANG AUS", "INPUT OFF"),
    RemoteControl => ("Fernsteuerung · LOCK erwartet", "Remote control · LOCK expected"),
    LocalControl => ("Lokal freigegeben · Messabfragen pausiert", "Local control released · polling paused"),
    ControlUnknown => ("LOCK / Bedienhoheit unbekannt", "LOCK / control ownership unknown"),
    LockHelp => ("Aus bestätigter Kommunikation abgeleitet; keine direkte LOCK-Abfrage am Gerät verfügbar.", "Inferred from confirmed communication; the device has no direct LOCK query."),
    ReleaseLocal => ("Lokal bedienen", "Use local controls"),
    ResumeRemote => ("Fernsteuerung fortsetzen", "Resume remote control"),
    Disconnect => ("Trennen", "Disconnect"),
    CommandRunning => ("Gerätebefehl läuft …", "Device command running …"),
    LocalHelp => ("Gerätetasten freigegeben. Messwerte und Einstellungen sind veraltet; beim Fortsetzen werden die Einstellungen neu gelesen. EINGANG AUS übernimmt wieder die Fernsteuerung.", "Device keys released. Measurements and settings are stale; resuming reads settings again. INPUT OFF takes back remote control."),
    BatteryCounters => ("Batterie · Gerätezähler", "Battery · device counters"),
    Capacity => ("Kapazität", "Capacity"),
    Energy => ("Energie", "Energy"),
    Stale => ("veraltet", "stale"),
    CountersHelp => ("Werte direkt vom Gerät; Zählerreset und Zuordnung zu einem einzelnen Lauf sind nicht bestätigt.", "Values come directly from the device; counter reset behavior and attribution to a single run are unverified."),
    ConnectForMeasurements => ("Gerät verbinden, um Messwerte und Live-Kurven anzuzeigen.", "Connect a device to display measurements and live curves."),
    MeasurementSettings => ("Messung & Aktualisierung", "Measurement & refresh"),
    PollPause => ("Messpause [s]", "Polling pause [s]"),
    PollHelp => ("Messpause gilt zusätzlich zur Dauer der Geräteabfragen (Batterie: zusätzlich Ah und Wh). Keine aufgestauten Messanfragen.", "The polling pause is added to the device query time (battery: also Ah and Wh). Measurement requests do not accumulate."),
    LiveCurves => ("Live-Kurven", "Live curves"),
    ClearCurves => ("Kurven leeren", "Clear curves"),
    HistoryHelp => ("Maximal 2000 echte Messpunkte · Zeitachse gemeinsam zoombar", "Up to 2000 actual measurements · linked time-axis zoom"),
    Measurements => ("Messwerte", "Measurements"),
    Unavailable => ("Veraltet / nicht verfügbar", "Stale / unavailable"),
    Voltage => ("Spannung", "Voltage"),
    Current => ("Strom", "Current"),
    Power => ("Leistung", "Power"),
    Resistance => ("Widerstand", "Resistance"),
    LastMeasurement => ("Letzte Messung", "Last measurement"),
    Time => ("Zeit", "Time"),
    BatteryUnknown => ("Verlauf unterbrochen / Zustand unbekannt", "Run interrupted / state unknown"),
    BatteryReady => ("Bereit · Eingang AUS", "Ready · input OFF"),
    BatteryActive => ("Entladung aktiv · Eingang EIN", "Discharge active · input ON"),
    BatteryEnded => ("Entladung beendet · Abschaltgrund unbekannt", "Discharge ended · cutoff reason unknown"),
    BatteryStopped => ("Gestoppt · Eingang AUS bestätigt", "Stopped · input OFF confirmed"),
    NotConnectedUnknown => ("Nicht verbunden – Eingangszustand unbekannt", "Not connected – input state unknown"),
    ConnectedRead => ("Verbunden; Eingangszustand vom Gerät gelesen", "Connected; input state read from device"),
    DisconnectedUnchanged => ("Getrennt – Eingang unverändert; Zustand unbekannt", "Disconnected – input unchanged; state unknown"),
    DisconnectedUnlocked => ("Getrennt – Entsperrbefehl gesendet (unbestätigt); Eingang unverändert", "Disconnected – unlock command sent (unconfirmed); input unchanged"),
    LocalReleased => ("Lokale Bedienung freigegeben; Messabfragen pausiert", "Local controls released; polling paused"),
    RemoteResumed => ("Fernsteuerung fortgesetzt; Geräteeinstellungen neu gelesen", "Remote control resumed; device settings read again"),
    SettingsRead => ("Geräteeinstellungen gelesen", "Device settings read"),
    SettingsApplied => ("Einstellungen angewendet und zurückgelesen; Eingang AUS", "Settings applied and read back; input OFF"),
    OnConfirmed => ("Eingang EIN bestätigt", "Input ON confirmed"),
    OffConfirmed => ("Eingang AUS bestätigt", "Input OFF confirmed"),
    BatteryApplied => ("Batterie-CC mit einer Stufe zurückgelesen; Eingang AUS", "Single-stage battery CC read back; input OFF"),
    CommandFailed => ("Gerätebefehl fehlgeschlagen", "Device command failed"),
    CommandFailedUnknown => ("Gerätebefehl fehlgeschlagen – Kanalstatus unbekannt", "Device command failed – channel state unknown"),
    CommunicationFailed => ("Kommunikationsfehler – Eingangszustand unbekannt", "Communication failed – input state unknown"),
    ConnectionLost => ("Verbindung verloren – Eingangszustand unbekannt", "Connection lost – input state unknown"),
    MeasurementFailed => ("Messung fehlgeschlagen – Eingangszustand unbekannt", "Measurement failed – input state unknown"),
    ModeChanged => ("Modus geändert – Geräteeinstellungen erneut lesen", "Mode changed – read device settings again"),
    QueueFull => ("Befehlswarteschlange voll; bitte warten", "Command queue full; please wait"),
    WorkerStopped => ("Geräte-Worker beendet", "Device worker stopped"),
    InvalidTimeout => ("Read-/Write-Timeout muss ein endlicher positiver Zeitwert sein", "Read/write timeout must be a finite positive duration"),
    InvalidPollPause => ("Messpause muss ein endlicher positiver Zeitwert sein", "Polling pause must be a finite positive duration"),
    PositiveCurrent => ("Entladestrom muss größer als 0 A sein", "discharge current must be greater than 0 A"),
    PollBounds => ("Messpause muss zwischen 0,5 und 60 s liegen", "poll pause must be 0.5..=60 s"),
    ResumeFirst => ("Vor Gerätebefehlen zuerst die Fernsteuerung fortsetzen", "resume remote control before sending device commands"),
    ReadBeforeOn => ("Vor EIN zuerst die Kanaleinstellungen lesen", "read channel settings before enabling"),
    SettingsChanged => ("Kanaleinstellungen geändert; erneut lesen und anwenden", "channel settings changed; read and apply again"),
    SingleStageRequired => ("Die GUI benötigt einstufiges Batterie-CC mit Spannungsabschaltung", "GUI requires single-stage battery CC with voltage cutoff"),
    BatteryOffRequired => ("Vor Entladestart muss der Eingang AUS sein", "battery input must be OFF before starting"),
    BatteryNotConfirmed => ("Einstufige Batteriekonfiguration / AUS nicht bestätigt", "single-stage battery configuration / OFF not confirmed"),
    BasicModesOnly => ("Die GUI unterstützt CC/CV/CP/CR", "GUI supports CC/CV/CP/CR"),
    ReadbackModeChanged => ("Modus während des Einstellungs-Readbacks geändert", "mode changed during configuration readback"),
    BatteryModeChanged => ("Modus während des Batterie-Readbacks geändert", "mode changed during battery readback"),
    NegativeCounters => ("Negative Batteriezähler", "negative battery counters"),
    InvalidParameter => ("Ungültiger Parameter", "Invalid parameter"),
    InvalidResponse => ("Ungültige Geräteantwort", "Invalid device response"),
    Unsupported => ("Nicht unterstützt", "Unsupported"),
    SerialError => ("Fehler am seriellen Port", "Serial port error"),
    DeviceRejected => ("Gerät hat den Befehl abgelehnt", "Device rejected the command"),
    ReconnectRequired => ("Verbindung nicht verfügbar; erneut verbinden – Eingangszustand unbekannt", "Session unavailable; reconnect required – input state unknown"),
    SwitchOffFailed => ("ABSCHALTEN FEHLGESCHLAGEN – EINGANGSZUSTAND UNBEKANNT", "SWITCH-OFF FAILED – INPUT STATE UNKNOWN"),
    MockError => ("Fehler im simulierten Transport", "Mock transport error"),
}

impl Text {
    pub fn english(self) -> &'static str {
        Language::English.text(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Text(Text),
    ChannelRead(u8),
    Error { summary: Text, detail: String },
}

impl Message {
    pub fn render(&self, language: Language) -> String {
        match self {
            Self::Text(text) => language.text(*text).into(),
            Self::ChannelRead(channel) => format!("{}: {}", language.channel(*channel), language.text(Text::SettingsRead)),
            Self::Error { summary, detail } => format!("{}: {detail}", language.text(*summary)),
        }
    }
}
impl From<Text> for Message {
    fn from(text: Text) -> Self {
        Self::Text(text)
    }
}
impl From<&et54::Error> for Message {
    fn from(error: &et54::Error) -> Self {
        use et54::Error;
        // GUI-owned validation messages use these same keys when creating driver errors.
        if let Error::Invalid(detail) | Error::Unsupported(detail) | Error::Protocol(detail) = error {
            for key in [
                Text::PositiveCurrent,
                Text::PollBounds,
                Text::ResumeFirst,
                Text::ReadBeforeOn,
                Text::SettingsChanged,
                Text::SingleStageRequired,
                Text::BatteryOffRequired,
                Text::BatteryNotConfirmed,
                Text::BasicModesOnly,
                Text::ReadbackModeChanged,
                Text::BatteryModeChanged,
                Text::NegativeCounters,
            ] {
                if detail == key.english() {
                    return key.into();
                }
            }
        }
        let summary = match error {
            Error::Io(_) => Text::CommunicationFailed,
            Error::Serial(_) => Text::SerialError,
            Error::Device { .. } => Text::DeviceRejected,
            Error::Protocol(_) => Text::InvalidResponse,
            Error::Invalid(_) => Text::InvalidParameter,
            Error::Unsupported(_) => Text::Unsupported,
            Error::Unavailable => Text::ReconnectRequired,
            Error::Mock(_) => Text::MockError,
            Error::DisableFailed { .. } => Text::SwitchOffFailed,
        };
        Self::Error {
            summary,
            detail: error.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "tests/i18n.rs"]
mod tests;
