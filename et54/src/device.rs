//! Instrument identity, lifetime and raw SCPI access.
use crate::{Channel, ConnectionConfig, Error, FanStatus, Model, Result, SerialTransport, Transport, scpi::Session};
use std::time::Duration;

/// Parsed *IDN? identity; retained alongside the original text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Effective model (possibly explicitly overridden).
    pub model: Model,
    /// Serial number; absent for Mustool's three-field identity.
    pub serial_number: Option<String>,
    /// Firmware version as reported, without assumptions about numbering.
    pub firmware: String,
    /// Hardware version as reported.
    pub hardware: String,
    /// Unmodified device identity response.
    pub raw: String,
}
impl DeviceInfo {
    /// Parse four whitespace/comma fields, or Mustool's XXXXXX firmware hardware.
    pub fn parse(raw: &str, model_override: Option<Model>) -> Result<Self> {
        if raw.contains(',')
            && raw
                .split(',')
                .any(|field| field.trim().is_empty())
        {
            return Err(Error::Protocol(format!("identity has empty fields: {raw:?}")));
        }
        let fields: Vec<_> = raw
            .trim()
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        let (name, serial, firmware, hardware) = match fields.as_slice() {
            [model, serial, firmware, hardware] => (*model, Some((*serial).into()), *firmware, *hardware),
            [model, firmware, hardware] if model.eq_ignore_ascii_case("XXXXXX") => (*model, None, *firmware, *hardware),
            _ => return Err(Error::Protocol(format!("identity: {raw:?}"))),
        };
        let model = match model_override {
            Some(m) => m,
            None => name.parse()?,
        };
        Ok(Self {
            model,
            serial_number: serial,
            firmware: firmware.into(),
            hardware: hardware.into(),
            raw: raw.into(),
        })
    }
}

/// One exclusively owned device connection. Dropping closes the transport, not the load.
pub struct Et54<T: Transport = SerialTransport> {
    pub(crate) session: Session<T>,
    info: DeviceInfo,
}

impl Et54<SerialTransport> {
    /// Open a serial port with default timing and identify; no configuration writes.
    pub fn open(path: &str) -> Result<Self> {
        Self::open_with_config(path, ConnectionConfig::default())
    }
    /// Open a serial port with explicit framing, timing and optional model override.
    pub fn open_with_config(path: &str, config: ConnectionConfig) -> Result<Self> {
        let transport = SerialTransport::open(path, &config)?;
        Self::connect(transport, config)
    }
}
impl<T: Transport> Et54<T> {
    /// Identify an already-open transport. No reset, SW ON/OFF or firmware command is sent.
    /// Late success ACKs are skipped only during this initial handshake, within one read timeout.
    pub fn connect(transport: T, config: ConnectionConfig) -> Result<Self> {
        config.validate()?;
        let mut session = Session {
            transport,
            config,
            available: true,
        };
        let info = match session
            .initial_identity()
            .and_then(|raw| DeviceInfo::parse(&raw, session.config.model_override))
        {
            Ok(info) => info,
            Err(e) => {
                let _ = session.transport.close();
                return Err(e);
            }
        };
        Ok(Self { session, info })
    }
    /// Identity cached at connection time; no I/O.
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }
    /// Query and refresh the complete identity. A changed model is rejected.
    pub fn identify(&mut self) -> Result<DeviceInfo> {
        let info = DeviceInfo::parse(&self.query_raw("*IDN?")?, self.session.config.model_override)?;
        if info.model != self.info.model {
            return Err(Error::Protocol("device model changed".into()));
        }
        self.info = info.clone();
        Ok(info)
    }
    /// Number of addressable input channels.
    pub fn channel_count(&self) -> u8 {
        self.info.model.channel_count()
    }
    /// Borrow a one-based channel exclusively; another channel cannot be used concurrently.
    pub fn channel(&mut self, number: u8) -> Result<Channel<'_, T>> {
        if number == 0 || number > self.channel_count() {
            return Err(Error::Invalid(format!("channel {number}")));
        }
        Ok(Channel { device: self, number })
    }
    /// Whether framing is still synchronized; not a physical connection probe.
    pub fn is_available(&self) -> bool {
        self.session.available
    }
    /// Direct SCPI read. Caller is responsible for command semantics, including side effects.
    pub fn query_raw(&mut self, command: &str) -> Result<String> {
        self.session.query(command)
    }
    /// Read one to ten response rows with a per-line timeout. No timeout mutation leaks.
    pub fn query_rows(&mut self, command: &str, rows: usize, timeout: Duration) -> Result<Vec<String>> {
        self.session
            .query_rows(command, rows, timeout)
    }
    /// Explicit raw write with ET54 ACK verification. Bypasses typed parameter checks.
    /// Use only for commands whose semantics and units you have verified.
    pub fn write_raw(&mut self, command: &str) -> Result<()> {
        self.session.write(command)
    }
    /// Sound the instrument buzzer.
    pub fn beep(&mut self) -> Result<()> {
        self.write_raw("SYST:BEEP")
    }
    /// Unlock local controls. Subsequent SCPI commands automatically lock them again.
    pub fn unlock(&mut self) -> Result<()> {
        self.write_raw("SYST:LOCA")
    }
    /// Lock local controls by issuing the read-only IDN command; no invented lock command.
    pub fn lock(&mut self) -> Result<()> {
        self.identify().map(|_| ())
    }
    /// Global software trigger using the documented *TRG spelling.
    pub fn trigger(&mut self) -> Result<()> {
        self.write_raw("*TRG")
    }
    /// Fan PASS/FAIL self-test status.
    pub fn fan(&mut self) -> Result<FanStatus> {
        self.query_raw("SELF:FAN?")?.parse()
    }
    /// Explicit factory reset. Reconnect afterwards, since
    /// reset timing and late responses are firmware-dependent. Never called on connect.
    pub fn reset(&mut self) -> Result<()> {
        self.session.send("RST")?;
        self.session.available = false;
        Ok(())
    }
    /// Validate and explicitly enable each channel sequentially. Earlier channels may be
    /// ON if a later activation fails; no atomic multi-channel operation exists.
    pub fn enable_all(&mut self) -> Result<()> {
        for n in 1..=self.channel_count() {
            self.channel(n)?.enable()?;
        }
        Ok(())
    }
    /// Attempt to disable every channel, returning the first failure.
    pub fn disable_all(&mut self) -> Result<()> {
        let mut first = None;
        for n in 1..=self.channel_count() {
            if let Err(e) = self.channel(n)?.disable()
                && first.is_none()
            {
                first = Some(e);
            }
        }
        match first {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    /// Close the host connection without switching the load input. Idempotent.
    pub fn close(&mut self) -> Result<()> {
        self.session.available = false;
        self.session.transport.close()
    }
    /// Read-only transport access, primarily for verifying consumed mock expectations.
    pub fn transport(&self) -> &T {
        &self.session.transport
    }
}
impl<T: Transport> Drop for Et54<T> {
    fn drop(&mut self) {
        let _ = self.session.transport.close();
    }
}
