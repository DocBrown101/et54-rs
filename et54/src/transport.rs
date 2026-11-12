//! Byte transports. SCPI framing and delays live above this layer.

use crate::{Error, Model, Result};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    time::Duration,
};

/// Serial and SCPI connection settings.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Host baud rate (does not change the instrument's configured baud rate).
    pub baud_rate: u32,
    /// Total deadline per response line.
    pub read_timeout: Duration,
    /// Timeout for sending an individual command.
    pub write_timeout: Duration,
    /// Exact response terminator, normally CRLF.
    pub read_termination: Vec<u8>,
    /// Command terminator, normally LF.
    pub write_termination: Vec<u8>,
    /// Pause after each complete response line; zero disables it. No pause after writing.
    pub operation_delay: Duration,
    /// Response-line timeout used for LIST queries.
    pub list_timeout: Duration,
    /// Explicit identity override, needed for Mustool's XXXXXX identity.
    pub model_override: Option<Model>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            baud_rate: 9600,
            read_timeout: Duration::from_secs(2),
            write_timeout: Duration::from_secs(2),
            read_termination: b"\r\n".to_vec(),
            write_termination: b"\n".to_vec(),
            operation_delay: Duration::from_millis(10),
            list_timeout: Duration::from_millis(2500),
            model_override: None,
        }
    }
}

impl ConnectionConfig {
    /// Validate framing and finite, nonzero timeout settings before opening a port.
    pub fn validate(&self) -> Result<()> {
        if self.baud_rate == 0
            || self.read_termination.is_empty()
            || self.write_termination.is_empty()
            || self.read_timeout.is_zero()
            || self.write_timeout.is_zero()
            || self.list_timeout.is_zero()
            || self.read_timeout > Duration::from_secs(60)
            || self.write_timeout > Duration::from_secs(60)
            || self.list_timeout > Duration::from_secs(60)
            || self.operation_delay > Duration::from_secs(10)
            || self.read_termination.len() > 8
            || self.write_termination.len() > 8
        {
            return Err(Error::Invalid("baud rate, terminators or timing out of bounds".into()));
        }
        Ok(())
    }
}

/// Exclusive, blocking byte-stream interface with bounded read/write operations.
pub trait Transport: Send {
    /// Send all bytes or return an error. Never retry a partial write here.
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;
    /// Read up to `buffer.len()` bytes; zero means disconnection/end of stream.
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize>;
    /// Set the deadline used for subsequent blocking calls.
    fn set_timeout(&mut self, timeout: Duration) -> Result<()>;
    /// Release the underlying resource. Wrappers may perform final cleanup first.
    fn close(&mut self) -> Result<()>;
}

/// Cross-platform USB serial transport (8 data bits, no parity, 1 stop bit).
pub struct SerialTransport {
    port: Option<Box<dyn serialport::SerialPort>>,
}

impl SerialTransport {
    /// Open a Linux device path, by-id symlink, or Windows COM port, clearing stale input.
    pub fn open(path: &str, config: &ConnectionConfig) -> Result<Self> {
        config.validate()?;
        let port = serialport::new(path, config.baud_rate)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(config.read_timeout)
            .open()?;
        port.clear(serialport::ClearBuffer::Input)?;
        Ok(Self { port: Some(port) })
    }
}

impl Transport for SerialTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.port
            .as_mut()
            .ok_or(Error::Unavailable)?
            .write_all(bytes)?;
        Ok(())
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        Ok(self
            .port
            .as_mut()
            .ok_or(Error::Unavailable)?
            .read(buffer)?)
    }
    fn set_timeout(&mut self, timeout: Duration) -> Result<()> {
        self.port
            .as_mut()
            .ok_or(Error::Unavailable)?
            .set_timeout(timeout)?;
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.port.take();
        Ok(())
    }
}

/// Discover serial devices without opening them. Linux additionally includes by-id paths.
pub fn available_ports() -> Result<Vec<String>> {
    let mut ports: Vec<_> = serialport::available_ports()?
        .into_iter()
        .map(|p| p.port_name)
        .collect();
    #[cfg(target_os = "linux")]
    if let Ok(entries) = std::fs::read_dir("/dev/serial/by-id") {
        ports.extend(
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path().to_string_lossy().into_owned()),
        );
    }
    ports.sort();
    ports.dedup();
    Ok(ports)
}

/// One expected wire operation; useful for timeouts, fragmentation and disconnects.
#[derive(Debug)]
pub enum MockStep {
    /// Exact expected command bytes, including termination.
    Write(Vec<u8>),
    /// Bytes returned by reads (may be consumed in fragments).
    Read(Vec<u8>),
    /// Inject a read error.
    ReadError(std::io::ErrorKind),
    /// Inject a write error.
    WriteError(std::io::ErrorKind),
}

/// Strict scripted mock: unexpected commands fail instead of returning dummy data.
#[derive(Debug, Default)]
pub struct MockTransport {
    steps: VecDeque<MockStep>,
    closed: bool,
    unexpected: Option<String>,
}

impl MockTransport {
    /// Construct an exact byte-level script.
    pub fn new(steps: impl IntoIterator<Item = MockStep>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            ..Default::default()
        }
    }
    /// Convenience script of LF commands and CRLF responses; multiline responses allowed.
    pub fn pairs(pairs: &[(&str, &str)]) -> Self {
        Self::new(
            pairs
                .iter()
                .flat_map(|(command, response)| {
                    [
                        MockStep::Write(format!("{command}\n").into_bytes()),
                        MockStep::Read(format!("{response}\r\n").into_bytes()),
                    ]
                }),
        )
    }
    /// Check that every expected operation was consumed and no unexpected operation occurred.
    ///
    /// # Panics
    ///
    /// Panics if an unexpected operation occurred or any scripted operation remains unconsumed.
    pub fn assert_finished(&self) {
        assert!(self.unexpected.is_none(), "unexpected mock operation: {:?}", self.unexpected);
        assert!(self.steps.is_empty(), "unconsumed mock operations: {:?}", self.steps);
    }
    /// Whether close was called.
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    fn unexpected(&mut self, message: String) -> Error {
        self.unexpected
            .get_or_insert(message.clone());
        Error::Mock(message)
    }
}

impl Transport for MockTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        if self.closed {
            return Err(Error::Unavailable);
        }
        match self.steps.pop_front() {
            Some(MockStep::Write(expected)) if expected == bytes => Ok(()),
            Some(MockStep::WriteError(kind)) => Err(std::io::Error::from(kind).into()),
            step => Err(self.unexpected(format!("write {bytes:?}, expected {step:?}"))),
        }
    }
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        if self.closed {
            return Err(Error::Unavailable);
        }
        match self.steps.pop_front() {
            Some(MockStep::Read(mut bytes)) => {
                let n = buffer.len().min(bytes.len());
                buffer[..n].copy_from_slice(&bytes[..n]);
                bytes.drain(..n);
                if !bytes.is_empty() {
                    self.steps
                        .push_front(MockStep::Read(bytes));
                }
                Ok(n)
            }
            Some(MockStep::ReadError(kind)) => Err(std::io::Error::from(kind).into()),
            step => Err(self.unexpected(format!("read, expected {step:?}"))),
        }
    }
    fn set_timeout(&mut self, _timeout: Duration) -> Result<()> {
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.closed = true;
        Ok(())
    }
}
