//! ET54 response parsing and serialized, timed SCPI transactions.

use crate::{ConnectionConfig, Error, Result, Transport};
use std::{
    thread,
    time::{Duration, Instant},
};

/// Remove an optional ET54 response prefix and surrounding whitespace.
pub fn payload(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('R')
        .unwrap_or(value)
        .trim()
}

/// Parse one finite floating-point response (optional leading R).
pub fn number(value: &str) -> Result<f64> {
    let n: f64 = payload(value)
        .parse()
        .map_err(|_| Error::Protocol(value.into()))?;
    if !n.is_finite() {
        return Err(Error::Protocol(value.into()));
    }
    Ok(n)
}

/// Parse one nonnegative integer response (optional leading R).
pub fn integer(value: &str) -> Result<u32> {
    payload(value)
        .parse()
        .map_err(|_| Error::Protocol(value.into()))
}

/// Parse comma- or whitespace-separated finite numbers, with optional leading R.
pub fn numbers(value: &str) -> Result<Vec<f64>> {
    let fields: Vec<_> = payload(value)
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if fields.is_empty()
        || payload(value)
            .split(',')
            .any(|s| s.trim().is_empty())
    {
        return Err(Error::Protocol(value.into()));
    }
    fields.into_iter().map(number).collect()
}

/// Validate a finite number in an inclusive range.
pub fn bounded(name: &str, value: f64, min: f64, max: f64) -> Result<()> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("{name}: {value}; expected {min}..={max}")))
    }
}

/// Extend one to three values to exactly three, repeating the last value.
pub fn expand_three(values: &[f64]) -> Result<[f64; 3]> {
    match values {
        [a] => Ok([*a; 3]),
        [a, b] => Ok([*a, *b, *b]),
        [a, b, c] => Ok([*a, *b, *c]),
        _ => Err(Error::Invalid("expected one to three values".into())),
    }
}

pub(crate) struct Session<T> {
    pub transport: T,
    pub config: ConnectionConfig,
    pub available: bool,
}

impl<T: Transport> Session<T> {
    pub(crate) fn send(&mut self, command: &str) -> Result<()> {
        if !self.available {
            return Err(Error::Unavailable);
        }
        if command.is_empty() || !command.is_ascii() || command.contains(['\r', '\n', ';']) {
            return Err(Error::Invalid("one ASCII SCPI command required; no CR/LF/semicolon".into()));
        }
        let mut bytes = command.as_bytes().to_vec();
        bytes.extend(&self.config.write_termination);
        let result = (|| {
            self.transport
                .set_timeout(self.config.write_timeout)?;
            self.transport.write_all(&bytes)
        })();
        if result.is_err() {
            self.available = false;
        }
        result
    }

    fn read_line(&mut self, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        let mut bytes = Vec::new();
        loop {
            let remaining = timeout
                .checked_sub(start.elapsed())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| Error::Io(std::io::ErrorKind::TimedOut.into()))?;
            self.transport.set_timeout(remaining)?;
            let mut byte = [0];
            if self.transport.read(&mut byte)? == 0 {
                return Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
            bytes.push(byte[0]);
            if bytes.ends_with(&self.config.read_termination) {
                bytes.truncate(bytes.len() - self.config.read_termination.len());
                let line = String::from_utf8(bytes).map_err(|e| Error::Protocol(e.to_string()))?;
                if line.trim().is_empty() {
                    return Err(Error::Protocol("empty response".into()));
                }
                return Ok(line);
            }
            if bytes.len() > 4096 {
                return Err(Error::Protocol("response exceeds 4096 bytes".into()));
            }
        }
    }

    fn read_response(&mut self, command: &str, timeout: Duration) -> Result<String> {
        let line = self.read_line(timeout)?;
        thread::sleep(self.config.operation_delay);
        if matches!(line.trim(), "Rcmd err" | "Rexecu err") {
            return Err(Error::Device {
                command: command.into(),
                response: line,
            });
        }
        Ok(line)
    }

    /// Only for connection setup: a late fire-and-forget LOCA ACK may arrive after opening.
    pub fn initial_identity(&mut self) -> Result<String> {
        self.send("*IDN?")?;
        let start = Instant::now();
        loop {
            let remaining = self
                .config
                .read_timeout
                .checked_sub(start.elapsed())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| Error::Io(std::io::ErrorKind::TimedOut.into()))?;
            let line = self.read_response("*IDN?", remaining)?;
            if line.trim() != "Rexecu success" {
                return Ok(line);
            }
        }
    }

    pub fn query_rows(&mut self, command: &str, rows: usize, timeout: Duration) -> Result<Vec<String>> {
        if rows == 0 || rows > 10 || timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(Error::Invalid("response rows must be 1..=10 and timeout 0..=60 seconds".into()));
        }
        self.send(command)?;
        let result = (|| {
            let mut values = Vec::with_capacity(rows);
            for _ in 0..rows {
                values.push(self.read_response(command, timeout)?);
            }
            Ok(values)
        })();
        // No retry: late frames cannot be associated with a subsequent command.
        if let Err(ref e) = result
            && (!matches!(e, Error::Device { .. }) || rows > 1)
        {
            self.available = false;
        }
        result
    }

    pub fn query(&mut self, command: &str) -> Result<String> {
        self.query_rows(command, 1, self.config.read_timeout)?
            .pop()
            .ok_or_else(|| Error::Protocol("missing response".into()))
    }

    pub fn write(&mut self, command: &str) -> Result<()> {
        let response = self.query(command)?;
        if response.trim() == "Rexecu success" {
            return Ok(());
        }
        self.available = false;
        Err(Error::Protocol(format!("{command:?}: unexpected ACK {response:?}")))
    }
}
