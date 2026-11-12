//! Typed protocol choices and complete special-mode configurations.
use crate::{Error, Result, scpi};
use std::{fmt, str::FromStr};

macro_rules! scpi_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $(#[doc = concat!("SCPI value `", $wire, "`.")] $variant),+ }
        impl $name {
            /// Canonical wire representation.
            pub const fn as_str(self) -> &'static str { match self { $(Self::$variant => $wire),+ } }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
        impl FromStr for $name {
            type Err = Error;
            fn from_str(value: &str) -> Result<Self> {
                let upper = value.trim().to_ascii_uppercase();
                match upper.as_str() {
                    $($wire => Ok(Self::$variant)),+,
                    _ => {
                        // Some firmware prefixes textual responses with R as well.
                        match upper.strip_prefix('R') {
                            $(Some($wire) => Ok(Self::$variant)),+,
                            _ => Err(Error::Protocol(format!("{}: {value:?}", stringify!($name)))),
                        }
                    }
                }
            }
        }
    }
}

scpi_enum! { /// Channel operating mode; qualification is an independent overlay.
    Mode { Cc => "CC", Cv => "CV", Cp => "CP", Cr => "CR", CcCv => "CCCV", CrCv => "CRCV",
        Short => "SHOR", Led => "LED", Battery => "BATT", Transient => "TRAN", List => "LIST", Scan => "SCAN" }
}
scpi_enum! { /// Voltage/current measurement range.
    Range { High => "HIGH", Low => "LOW" }
}
scpi_enum! { /// Channel input state as actually reported by the instrument.
    InputState { On => "ON", Off => "OFF" }
}
scpi_enum! { /// Protection and load-regulation status.
    Protection { None => "NONE", OverVoltage => "OV", OverCurrent => "OC", OverPower => "OP",
        OverTemperature => "OT", ReverseVoltage => "LRV", FanFailure => "FAN", Unregulated => "UN", Failure => "FAIL" }
}
scpi_enum! { /// Fan self-test status, not fan rotation speed.
    FanStatus { Pass => "PASS", Fail => "FAIL" }
}
scpi_enum! { /// Trigger source.
    TriggerMode { Manual => "MAN", External => "EXT", Software => "TRG", All => "ALL" }
}
scpi_enum! { /// Battery discharge submode.
    BatterySubmode { Cc => "CC", Cr => "CR" }
}
scpi_enum! { /// Transient regulation submode.
    TransientSubmode { Cc => "CC", Cv => "CV" }
}
scpi_enum! { /// Transient trigger mode. Continuous uses the instrument's spelling COUT.
    TransientTrigger { Continuous => "COUT", Trigger => "TRIG", Pulse => "PULS" }
}
scpi_enum! { /// LIST advancement source.
    ListStepMode { Auto => "AUTO", Trigger => "TRIGGER" }
}
scpi_enum! { /// LIST regulation mode.
    ListMode { Cc => "CC", Cv => "CV", Cp => "CP", Cr => "CR", Open => "OPEN", Short => "SHORT" }
}
scpi_enum! { /// Quantity compared in a LIST row.
    Comparison { Off => "OFF", Current => "CURRENT", Voltage => "VOLTAGE", Power => "POWER", Resistance => "RESISTANCE" }
}
scpi_enum! { /// Qualification or LIST comparison outcome.
    TestResult { None => "NONE", Pass => "PASS", Fail => "FAIL" }
}
scpi_enum! { /// SCAN regulation submode.
    ScanSubmode { Cc => "CC", Cv => "CV", Cp => "CP" }
}
scpi_enum! { /// SCAN threshold type. DROP has no documented numeric value command.
    ScanThreshold { Voltage => "VTH", Drop => "DROP", MinimumVoltage => "VMIN" }
}
scpi_enum! { /// SCAN comparison.
    ScanComparison { Current => "INCURR", Voltage => "INVOLT", Power => "INPOW", Off => "OFF", None => "NONE" }
}

impl ListMode {
    /// Integer code used by LIST:PARA and LIST:OUT.
    pub fn code(self) -> u8 {
        self as u8
    }
    /// Decode a LIST mode index, rejecting unknown indices.
    pub fn from_code(code: u32) -> Result<Self> {
        [Self::Cc, Self::Cv, Self::Cp, Self::Cr, Self::Open, Self::Short]
            .get(code as usize)
            .copied()
            .ok_or_else(|| Error::Protocol(format!("LIST mode index {code}")))
    }
}
impl Comparison {
    /// Integer code used by LIST:PARA.
    pub fn code(self) -> u8 {
        self as u8
    }
    /// Decode a comparison index.
    pub fn from_code(code: u32) -> Result<Self> {
        [Self::Off, Self::Current, Self::Voltage, Self::Power, Self::Resistance]
            .get(code as usize)
            .copied()
            .ok_or_else(|| Error::Protocol(format!("comparison index {code}")))
    }
}

/// A base-mode configuration. Setting it never enables the input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoadMode {
    /// Constant current in amperes.
    ConstantCurrent(f64),
    /// Constant voltage in volts.
    ConstantVoltage(f64),
    /// Constant power in watts.
    ConstantPower(f64),
    /// Constant resistance in ohms.
    ConstantResistance(f64),
    /// Combined CC+CV mode.
    CcCv {
        /// Current in amperes.
        current: f64,
        /// Voltage in volts.
        voltage: f64,
    },
    /// Combined CR+CV mode.
    CrCv {
        /// Resistance in ohms.
        resistance: f64,
        /// Voltage in volts.
        voltage: f64,
    },
    /// Short-circuit mode; activation is still explicit.
    ShortCircuit,
    /// LED simulation: Rd = voltage/current * coefficient; Vf = voltage*(1-coefficient).
    Led {
        /// V0 in volts.
        voltage: f64,
        /// I0 in amperes.
        current: f64,
        /// Dimensionless coefficient, 0.01..=1.
        coefficient: f64,
    },
}

impl LoadMode {
    /// Operating mode selected by this configuration.
    pub fn mode(self) -> Mode {
        match self {
            Self::ConstantCurrent(_) => Mode::Cc,
            Self::ConstantVoltage(_) => Mode::Cv,
            Self::ConstantPower(_) => Mode::Cp,
            Self::ConstantResistance(_) => Mode::Cr,
            Self::CcCv { .. } => Mode::CcCv,
            Self::CrCv { .. } => Mode::CrCv,
            Self::ShortCircuit => Mode::Short,
            Self::Led { .. } => Mode::Led,
        }
    }
}

/// Battery cutoff selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryCutoff {
    /// Volts, one stage for CR / three stages for CC.
    Voltage,
    /// Seconds.
    Time,
    /// Ampere-hours.
    Capacity,
    /// Watt-hours.
    Energy,
}
impl BatteryCutoff {
    /// Canonical write parameter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Voltage => "V",
            Self::Time => "T",
            Self::Capacity => "C",
            Self::Energy => "E",
        }
    }
}
impl FromStr for BatteryCutoff {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self> {
        match scpi::payload(value)
            .to_ascii_uppercase()
            .as_str()
        {
            "V" | "VOLTAGE" => Ok(Self::Voltage),
            "T" | "TIME" => Ok(Self::Time),
            "C" | "CAPACITY" => Ok(Self::Capacity),
            "E" | "ENERGY" => Ok(Self::Energy),
            _ => Err(Error::Protocol(format!("battery cutoff {value:?}"))),
        }
    }
}

/// One scalar or three CC/voltage battery-stage values, in wire register order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryValues {
    /// Single current, resistance, time, voltage, capacity or energy value.
    Single(f64),
    /// BCC1, BCC2, BCC3 values; do not reorder according to voltage magnitude.
    Stages([f64; 3]),
}
impl BatteryValues {
    /// Repeat a scalar to three stages, or return existing stages unchanged.
    pub fn stages(self) -> [f64; 3] {
        match self {
            Self::Single(x) => [x; 3],
            Self::Stages(x) => x,
        }
    }
    /// Require a scalar (non-voltage cutoff or CR discharge).
    pub fn single(self) -> Result<f64> {
        match self {
            Self::Single(x) => Ok(x),
            Self::Stages(_) => Err(Error::Invalid("scalar required".into())),
        }
    }
}

/// Complete battery configuration; currents in A, resistance in ohms, cutoffs per selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryConfig {
    /// CC or CR discharge.
    pub submode: BatterySubmode,
    /// Discharge setpoint, three values only for CC with voltage cutoff.
    pub value: BatteryValues,
    /// Stop condition.
    pub cutoff: BatteryCutoff,
    /// Cutoff in V / s / Ah / Wh.
    pub cutoff_value: BatteryValues,
}

/// Transient setup. Values and widths are A/B register order, not sorted low/high.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransientConfig {
    /// CC or CV.
    pub submode: TransientSubmode,
    /// Continuous, triggered or pulsed switching.
    pub trigger: TransientTrigger,
    /// Two current (A) or voltage (V) setpoints.
    pub values: [f64; 2],
    /// Pulse widths in **milliseconds**, 50..=model limit.
    pub width_ms: [u32; 2],
}

/// A LIST row to write. Unlike readback, all numeric values must be valid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListRow {
    /// Row number, 1..=10.
    pub number: u8,
    /// Load mode for this step.
    pub mode: ListMode,
    /// A / V / W / ohms according to mode (ignored by hardware in OPEN/SHORT).
    pub value: f64,
    /// Step duration in seconds, 1..=60000.
    pub delay_s: u32,
    /// Compared quantity, or disabled.
    pub comparison: Comparison,
    /// Comparison upper limit.
    pub max: f64,
    /// Comparison lower limit.
    pub min: f64,
}

/// LIST parameter readback; `---` and other explicit unavailable markers are None.
#[derive(Debug, Clone, PartialEq)]
pub struct ListRowReadback {
    /// Row number.
    pub number: u8,
    /// Step mode.
    pub mode: ListMode,
    /// Setpoint if available.
    pub value: Option<f64>,
    /// Step duration in seconds.
    pub delay_s: u32,
    /// Comparison selector.
    pub comparison: Comparison,
    /// Upper limit if available.
    pub max: Option<f64>,
    /// Lower limit if available.
    pub min: Option<f64>,
}

/// Completed LIST step result, not a fabricated overall pass/fail.
#[derive(Debug, Clone, PartialEq)]
pub struct ListResult {
    /// Step number.
    pub number: u8,
    /// Step mode.
    pub mode: ListMode,
    /// Setpoint, if reported.
    pub value: Option<f64>,
    /// None corresponds to the device's comparison-disabled/NA state (code 0).
    pub result: TestResult,
    /// Reported maximum, if available.
    pub max: Option<f64>,
    /// Reported minimum, if available.
    pub min: Option<f64>,
}

pub(crate) fn optional_number(s: &str) -> Result<Option<f64>> {
    match s.trim() {
        "---" | "NA" => Ok(None),
        _ => scpi::number(s).map(Some),
    }
}
pub(crate) fn list_fields(s: &str, count: usize) -> Result<Vec<&str>> {
    let fields: Vec<_> = scpi::payload(s)
        .split(',')
        .map(str::trim)
        .collect();
    if fields.len() == count {
        Ok(fields)
    } else {
        Err(Error::Protocol(format!("LIST fields: {s:?}")))
    }
}
pub(crate) fn row_number(s: &str) -> Result<u8> {
    let n = scpi::integer(s)?;
    if (1..=10).contains(&n) {
        Ok(n as u8)
    } else {
        Err(Error::Protocol(format!("row {n}")))
    }
}
impl ListRowReadback {
    /// Parse a seven-field LIST:PARA response, rejecting invalid enum indices.
    pub fn parse(s: &str) -> Result<Self> {
        let f = list_fields(s, 7)?;
        Ok(Self {
            number: row_number(f[0])?,
            mode: ListMode::from_code(scpi::integer(f[1])?)?,
            value: optional_number(f[2])?,
            delay_s: scpi::integer(f[3])?,
            comparison: Comparison::from_code(scpi::integer(f[4])?)?,
            max: optional_number(f[5])?,
            min: optional_number(f[6])?,
        })
    }
}
impl ListResult {
    /// Parse the six-field form used by the original implementation.
    /// The PDF's inconsistent seven-field example is deliberately rejected.
    pub fn parse(s: &str) -> Result<Self> {
        let f = list_fields(s, 6)?;
        let result = match scpi::integer(f[3])? {
            0 => TestResult::None,
            1 => TestResult::Pass,
            2 => TestResult::Fail,
            n => return Err(Error::Protocol(format!("LIST result index {n}"))),
        };
        Ok(Self {
            number: row_number(f[0])?,
            mode: ListMode::from_code(scpi::integer(f[1])?)?,
            value: optional_number(f[2])?,
            result,
            max: optional_number(f[4])?,
            min: optional_number(f[5])?,
        })
    }
}

/// SCAN configuration. DROP's numeric threshold is unsupported, not silently ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanConfig {
    /// CC, CV or CP regulation.
    pub submode: ScanSubmode,
    /// Threshold selector.
    pub threshold: ScanThreshold,
    /// Voltage threshold, VTH or VMIN, in volts; None for DROP.
    pub threshold_value: Option<f64>,
    /// Comparison selector.
    pub comparison: ScanComparison,
    /// Lower/upper comparison bounds.
    pub limits: [f64; 2],
    /// Start/end setpoints, in submode units.
    pub start_end: [f64; 2],
    /// Positive step increment.
    pub step: f64,
    /// Step dwell in seconds, 1..=999.
    pub step_delay_s: u32,
}

/// Qualification comparison overlay for CC/CV/CP/CR.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualificationConfig {
    /// Voltage lower/upper bound in V.
    pub voltage: [f64; 2],
    /// Current lower/upper bound in A.
    pub current: [f64; 2],
    /// Power lower/upper bound in W.
    pub power: [f64; 2],
}
