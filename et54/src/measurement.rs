//! Measurement values in SI units.
use crate::{Error, Result, scpi};

/// A single MEAS:ALL sample; wire order is current, voltage, power, resistance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurements {
    /// Input voltage in volts.
    pub voltage: f64,
    /// Input current in amperes.
    pub current: f64,
    /// Dissipated power in watts.
    pub power: f64,
    /// Equivalent resistance in ohms; None for explicitly recognized unavailable/overrange tokens.
    pub resistance: Option<f64>,
}

impl Measurements {
    /// Parse the exact four-field device response, with optional R and commas.
    pub fn parse(response: &str) -> Result<Self> {
        let fields: Vec<_> = scpi::payload(response)
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        if fields.len() != 4
            || scpi::payload(response)
                .split(',')
                .any(|s| s.trim().is_empty())
        {
            return Err(Error::Protocol(response.into()));
        }
        let resistance = match fields[3].to_ascii_uppercase().as_str() {
            "---" | "OL" | "OVER" | "INF" | "+INF" => None,
            _ => Some(scpi::number(fields[3])?),
        };
        Ok(Self {
            current: scpi::number(fields[0])?,
            voltage: scpi::number(fields[1])?,
            power: scpi::number(fields[2])?,
            resistance,
        })
    }
}
