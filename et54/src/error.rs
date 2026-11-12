//! Structured errors; no error implies that a load has been switched off.

/// Driver result.
pub type Result<T> = std::result::Result<T, Error>;

/// Device, protocol, validation or transport failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Operating-system I/O error.
    #[error("communication failed (input state unknown): {0}")]
    Io(#[from] std::io::Error),
    /// Serial port configuration or discovery failure.
    #[error("serial port: {0}")]
    Serial(#[from] serialport::Error),
    /// Complete device error response, including the rejected command.
    #[error("device rejected {command:?}: {response}")]
    Device {
        /// Sent command.
        command: String,
        /// Complete device response.
        response: String,
    },
    /// Malformed or unexpected data.
    #[error("invalid response: {0}")]
    Protocol(String),
    /// Invalid argument; rejected before the corresponding write.
    #[error("invalid parameter: {0}")]
    Invalid(String),
    /// Unsupported model or documented feature limitation.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Transport was closed, or framing was lost; reconnect is required.
    #[error("session unavailable; reconnect required (input state unknown)")]
    Unavailable,
    /// A mock expectation did not match the actual wire operation.
    #[error("mock transport: {0}")]
    Mock(String),
    /// OFF was not confirmed; do not infer a safe hardware state.
    #[error("SWITCH-OFF FAILED for channel {channel}; INPUT STATE UNKNOWN: {source}")]
    DisableFailed {
        /// Affected channel.
        channel: u8,
        /// Underlying error.
        source: Box<Error>,
    },
}
