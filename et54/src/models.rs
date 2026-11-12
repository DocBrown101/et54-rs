//! Supported identities and conservative, documented model limits.
use crate::{Error, Mode, Result};
use std::{fmt, str::FromStr};

/// Models recognized by ET54.py (including A+ variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// Single-channel 120 V model.
    Et5406APlus,
    /// Single-channel 180 V model.
    Et5407APlus,
    /// Single-channel 150 V model.
    Et5410,
    /// A+ variant of ET5410.
    Et5410APlus,
    /// Single-channel 500 V model.
    Et5411,
    /// A+ variant of ET5411.
    Et5411APlus,
    /// Two-channel 150 V model.
    Et5420,
    /// A+ variant of ET5420.
    Et5420APlus,
}

/// Model-dependent high-range limits, per channel.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Maximum voltage setpoint in volts.
    pub voltage: f64,
    /// Maximum current setpoint in amperes (conservative for 5406/5407).
    pub current: f64,
    /// Maximum power setpoint in watts.
    pub power: f64,
    /// Maximum resistance in ohms.
    pub resistance: f64,
    /// Maximum OVP setting in volts.
    pub ovp: f64,
    /// Maximum OCP setting in amperes.
    pub ocp: f64,
    /// Maximum OPP setting in watts.
    pub opp: f64,
    /// Maximum transient width in milliseconds / battery timer in seconds.
    pub time: u32,
}

impl Model {
    /// All supported model identifiers.
    pub const ALL: [Self; 8] = [
        Self::Et5406APlus,
        Self::Et5407APlus,
        Self::Et5410,
        Self::Et5410APlus,
        Self::Et5411,
        Self::Et5411APlus,
        Self::Et5420,
        Self::Et5420APlus,
    ];
    /// Physical channel count; no probing commands required.
    pub fn channel_count(self) -> u8 {
        if matches!(self, Self::Et5420 | Self::Et5420APlus) { 2 } else { 1 }
    }
    /// Documented capability; 5406/5407 exclude CCCV, CRCV and LED.
    pub fn supports(self, mode: Mode) -> bool {
        !matches!(self, Self::Et5406APlus | Self::Et5407APlus) || !matches!(mode, Mode::CcCv | Mode::CrCv | Mode::Led)
    }
    /// Conservative software limits; not electrically verified on hardware.
    pub fn limits(self) -> Limits {
        let (voltage, current, power, ovp, ocp, opp) = match self {
            Self::Et5406APlus => (120., 20., 200., 125., 21., 210.),
            Self::Et5407APlus => (180., 20., 200., 180., 20., 210.),
            Self::Et5410 | Self::Et5410APlus => (150., 40., 400., 155., 45., 420.),
            Self::Et5411 | Self::Et5411APlus => (500., 15., 400., 520., 16., 420.),
            Self::Et5420 | Self::Et5420APlus => (150., 20., 200., 155., 22., 220.),
        };
        let small = matches!(self, Self::Et5406APlus | Self::Et5407APlus);
        Limits {
            voltage,
            current,
            power,
            ovp,
            ocp,
            opp,
            resistance: if small { 4500. } else { 5000. },
            time: if small { 99999 } else { 60000 },
        }
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Et5406APlus => "ET5406A+",
            Self::Et5407APlus => "ET5407A+",
            Self::Et5410 => "ET5410",
            Self::Et5410APlus => "ET5410A+",
            Self::Et5411 => "ET5411",
            Self::Et5411APlus => "ET5411A+",
            Self::Et5420 => "ET5420",
            Self::Et5420APlus => "ET5420A+",
        })
    }
}
impl FromStr for Model {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|m| {
                m.to_string()
                    .eq_ignore_ascii_case(s.trim())
            })
            .ok_or_else(|| Error::Unsupported(format!("model {s:?}; use explicit override for XXXXXX")))
    }
}
