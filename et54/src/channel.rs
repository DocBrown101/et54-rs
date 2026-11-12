//! Per-channel control, validation and actual-state readback.
use crate::scpi::{self, bounded};
use crate::{
    BatterySubmode, BatteryValues, Comparison, Error, Et54, InputState, ListMode, ListRow, LoadMode, Measurements, Mode, Model, Protection,
    QualificationConfig, Range, Result, SerialTransport, TestResult, TransientSubmode, Transport, TriggerMode,
};

/// Configured protection thresholds, in V / A / W.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProtectionLimits {
    /// Overvoltage threshold in volts.
    pub voltage: f64,
    /// Overcurrent threshold in amperes.
    pub current: f64,
    /// Overpower threshold in watts.
    pub power: f64,
}

/// Input start/stop voltage thresholds, independent of the input switch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoltageThresholds {
    /// V-on start voltage in volts.
    pub on: f64,
    /// V-off shutdown voltage in volts.
    pub off: f64,
}

impl VoltageThresholds {
    /// Validate both voltages against the model and voltage range without I/O.
    pub fn validate(self, model: Model, range: Range) -> Result<()> {
        let max = if range == Range::Low { 20. } else { model.limits().voltage };
        bounded("V-on [V]", self.on, 0.1, max)?;
        bounded("V-off [V]", self.off, 0.1, max)
    }
}

/// Snapshot of actual channel state, not cached GUI intent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelStatus {
    /// Input switch readback.
    pub input: InputState,
    /// Operating mode readback.
    pub mode: Mode,
    /// Protection/regulation state.
    pub protection: Protection,
}

/// Exclusively borrowed channel handle. Dropping it does not switch the input off.
pub struct Channel<'a, T: Transport = SerialTransport> {
    pub(crate) device: &'a mut Et54<T>,
    pub(crate) number: u8,
}

#[derive(Clone, Copy)]
enum Kind {
    Voltage,
    VoltageComparison,
    Current,
    Power,
    Resistance,
    BatteryResistance,
    Ovp,
    Ocp,
    Opp,
    Coefficient,
    Capacity,
    Time,
    Width,
    StepDelay,
}

impl Kind {
    fn validate(self, model: Model, value: f64) -> Result<()> {
        let l = model.limits();
        let (name, min, max) = match self {
            Kind::Voltage => ("voltage [V]", 0.1, l.voltage),
            Kind::VoltageComparison => ("voltage comparison [V]", 0., l.voltage),
            Kind::Current => ("current [A]", 0., l.current),
            Kind::Power => ("power [W]", 0., l.power),
            Kind::Resistance => ("resistance [ohm]", 0.01, l.resistance),
            Kind::BatteryResistance => ("battery resistance [ohm]", 0.03, 4500.),
            Kind::Ovp => ("OVP [V]", 0.1, l.ovp),
            Kind::Ocp => ("OCP [A]", 0., l.ocp),
            Kind::Opp => ("OPP [W]", 0., l.opp),
            Kind::Coefficient => ("LED coefficient", 0.01, 1.),
            Kind::Capacity => ("Ah/Wh", 0., 9999.),
            Kind::Time => ("time [s]", 1., l.time as f64),
            Kind::Width => ("width [ms]", 50., l.time as f64),
            Kind::StepDelay => ("step delay [s]", 1., 999.),
        };
        bounded(name, value, min, max)
    }
}

impl ProtectionLimits {
    fn validate(self, model: Model) -> Result<()> {
        Kind::Ovp.validate(model, self.voltage)?;
        Kind::Ocp.validate(model, self.current)?;
        Kind::Opp.validate(model, self.power)
    }

    fn validate_activation(self, model: Model, ranges: [Range; 2]) -> Result<()> {
        self.validate(model)?;
        if self.current <= 0. || self.power <= 0. {
            return Err(Error::Invalid("positive OCP and OPP required for activation".into()));
        }
        if (ranges[0] == Range::Low && self.voltage > 21.) || (ranges[1] == Range::Low && self.current > 3.3) {
            return Err(Error::Invalid("protection exceeds selected low range".into()));
        }
        Ok(())
    }

    fn validate_setpoint(self, model: Model, ranges: [Range; 2], kind: Kind, value: f64) -> Result<()> {
        kind.validate(model, value)?;
        let max = match kind {
            Kind::Voltage => self
                .voltage
                .min(if ranges[0] == Range::Low { 20. } else { model.limits().voltage }),
            Kind::Current => self
                .current
                .min(if ranges[1] == Range::Low { 3. } else { model.limits().current }),
            Kind::Power => self.power,
            _ => f64::MAX,
        };
        if value > max {
            return Err(Error::Invalid(format!("setpoint {value} exceeds protection/range {max}")));
        }
        Ok(())
    }
}

impl LoadMode {
    /// Validate model support, setpoints, ranges and protection limits without I/O.
    /// Uses the same numeric checks as activation; actual device state is still read before ON.
    pub fn validate(self, model: Model, protections: ProtectionLimits, ranges: [Range; 2]) -> Result<()> {
        if !model.supports(self.mode()) {
            return Err(Error::Unsupported(format!("{model} mode {}", self.mode())));
        }
        protections.validate_activation(model, ranges)?;
        for (kind, value) in self.setpoint_values() {
            protections.validate_setpoint(model, ranges, kind, value)?;
        }
        Ok(())
    }

    fn setpoint_values(self) -> Vec<(Kind, f64)> {
        match self {
            LoadMode::ConstantCurrent(v) => vec![(Kind::Current, v)],
            LoadMode::ConstantVoltage(v) => vec![(Kind::Voltage, v)],
            LoadMode::ConstantPower(v) => vec![(Kind::Power, v)],
            LoadMode::ConstantResistance(v) => vec![(Kind::Resistance, v)],
            LoadMode::CcCv { current, voltage } => {
                vec![(Kind::Current, current), (Kind::Voltage, voltage)]
            }
            LoadMode::CrCv { resistance, voltage } => vec![(Kind::Resistance, resistance), (Kind::Voltage, voltage)],
            LoadMode::Led { voltage, current, coefficient } => {
                vec![(Kind::Voltage, voltage), (Kind::Current, current), (Kind::Coefficient, coefficient)]
            }
            LoadMode::ShortCircuit => vec![],
        }
    }
}

macro_rules! scalar {
    ($get:ident, $set:ident, $system:literal, $parameter:literal, $kind:ident, $doc:literal) => {
        #[doc = concat!("Read ", $doc, ".")]
        pub fn $get(&mut self) -> Result<f64> {
            self.query_float($system, $parameter)
        }
        #[doc = concat!("Set ", $doc, "; finite, model-bounded values only. Does not enable input.")]
        pub fn $set(&mut self, value: f64) -> Result<()> {
            self.set_float($system, $parameter, value, Kind::$kind)
        }
    };
}
macro_rules! choice {
    ($get:ident, $set:ident, $system:literal, $parameter:literal, $type:ty, $doc:literal) => {
        #[doc = concat!("Read ", $doc, ".")]
        pub fn $get(&mut self) -> Result<$type> {
            self.query_parameter($system, $parameter)?
                .parse()
        }
        #[doc = concat!("Set ", $doc, ". Does not activate the input.")]
        pub fn $set(&mut self, value: $type) -> Result<()> {
            self.write_parameter($system, $parameter, value)
        }
    };
}

mod battery;
mod list;
mod scan;
mod transient;

impl<T: Transport> Channel<'_, T> {
    /// One-based channel address.
    pub fn number(&self) -> u8 {
        self.number
    }
    /// Model associated with this channel.
    pub fn model(&self) -> Model {
        self.device.info().model
    }
    fn build_channel_command(&self, system: &str, parameter: &str) -> String {
        format!("{system}{}:{parameter}", self.number)
    }
    fn query_parameter(&mut self, system: &str, parameter: &str) -> Result<String> {
        self.device
            .query_raw(&format!("{}?", self.build_channel_command(system, parameter)))
    }
    fn write_parameter(&mut self, system: &str, parameter: &str, value: impl std::fmt::Display) -> Result<()> {
        self.device
            .write_raw(&format!("{} {value}", self.build_channel_command(system, parameter)))
    }
    fn query_float(&mut self, system: &str, parameter: &str) -> Result<f64> {
        scpi::number(&self.query_parameter(system, parameter)?)
    }
    fn validate(&self, value: f64, kind: Kind) -> Result<()> {
        kind.validate(self.model(), value)
    }
    fn set_float(&mut self, system: &str, parameter: &str, value: f64, kind: Kind) -> Result<()> {
        self.validate(value, kind)?;
        // Combined/LED registers are absent from the 5406/5407 protocol.
        let required = match parameter {
            "CCCV" => Some(Mode::CcCv),
            "CRCV" => Some(Mode::CrCv),
            "LED" | "COEF" => Some(Mode::Led),
            _ => None,
        };
        if let Some(mode) = required {
            self.require_mode(mode)?;
        }
        self.write_parameter(system, parameter, value)
    }
    fn require_mode(&self, mode: Mode) -> Result<()> {
        if self.model().supports(mode) {
            Ok(())
        } else {
            Err(Error::Unsupported(format!("{} mode {mode}", self.model())))
        }
    }
    fn query_float_pair(&mut self, system: &str, a: &str, b: &str) -> Result<[f64; 2]> {
        Ok([self.query_float(system, a)?, self.query_float(system, b)?])
    }
    fn validate_pair(&self, values: [f64; 2], kind: Kind, ordered: bool) -> Result<()> {
        for v in values {
            self.validate(v, kind)?;
        }
        if ordered && values[0] > values[1] {
            return Err(Error::Invalid("lower bound exceeds upper bound".into()));
        }
        Ok(())
    }
    fn set_pair(&mut self, system: &str, a: &str, b: &str, values: [f64; 2], kind: Kind, ordered: bool) -> Result<()> {
        self.validate_pair(values, kind, ordered)?;
        self.write_parameter(system, a, values[0])?;
        self.write_parameter(system, b, values[1])
    }

    /// Actual input state; errors must be treated as unknown, not OFF.
    pub fn input(&mut self) -> Result<InputState> {
        self.query_parameter("Ch", "SW")?
            .parse()
    }
    /// Set the input explicitly; ON always runs the readback preflight.
    pub fn set_input(&mut self, state: InputState) -> Result<()> {
        match state {
            InputState::On => self.enable(),
            InputState::Off => self.disable(),
        }
    }
    /// Explicitly enable after validating the actual mode, all active setpoints,
    /// ranges and protection thresholds. No write retry is performed.
    pub fn enable(&mut self) -> Result<()> {
        self.validate_activation()?;
        self.write_parameter("Ch", "SW", InputState::On)?;
        if self.input()? != InputState::On {
            return Err(Error::Protocol("input activation not confirmed".into()));
        }
        Ok(())
    }
    /// Disable and verify OFF readback. Every failure explicitly reports unknown state.
    pub fn disable(&mut self) -> Result<()> {
        let result = (|| {
            self.write_parameter("Ch", "SW", InputState::Off)?;
            if self.input()? != InputState::Off {
                return Err(Error::Protocol("OFF readback not confirmed".into()));
            }
            Ok(())
        })();
        result.map_err(|source| Error::DisableFailed {
            channel: self.number,
            source: Box::new(source),
        })
    }
    /// Actual channel operating mode.
    pub fn mode(&mut self) -> Result<Mode> {
        self.query_parameter("Ch", "MODE")?
            .parse()
    }
    /// Select a mode using existing setpoints. Input remains controlled by the device;
    /// switching modes is not an activation and may turn the device input off.
    pub fn select_mode(&mut self, mode: Mode) -> Result<()> {
        self.require_mode(mode)?;
        self.write_parameter("Ch", "MODE", mode)
    }
    /// Configure a basic, combined, short or LED mode, writing parameters before mode.
    /// All arguments are checked before any write, including compound configurations.
    pub fn set_mode(&mut self, config: LoadMode) -> Result<()> {
        self.require_mode(config.mode())?;
        for (kind, value) in config.setpoint_values() {
            self.validate(value, kind)?;
        }
        match config {
            LoadMode::ConstantCurrent(v) => self.set_cc_current(v)?,
            LoadMode::ConstantVoltage(v) => self.set_cv_voltage(v)?,
            LoadMode::ConstantPower(v) => self.set_cp_power(v)?,
            LoadMode::ConstantResistance(v) => self.set_cr_resistance(v)?,
            LoadMode::CcCv { current, voltage } => {
                self.set_cccv_current(current)?;
                self.set_cccv_voltage(voltage)?;
            }
            LoadMode::CrCv { resistance, voltage } => {
                self.set_crcv_voltage(voltage)?;
                self.set_crcv_resistance(resistance)?;
            }
            LoadMode::Led { voltage, current, coefficient } => {
                self.set_led_voltage(voltage)?;
                self.set_led_current(current)?;
                self.set_led_coefficient(coefficient)?;
            }
            LoadMode::ShortCircuit => {}
        }
        self.select_mode(config.mode())
    }
    /// Read the actual basic/combined/LED setpoints; special modes have separate getters.
    pub fn load_mode(&mut self) -> Result<LoadMode> {
        match self.mode()? {
            Mode::Cc => Ok(LoadMode::ConstantCurrent(self.cc_current()?)),
            Mode::Cv => Ok(LoadMode::ConstantVoltage(self.cv_voltage()?)),
            Mode::Cp => Ok(LoadMode::ConstantPower(self.cp_power()?)),
            Mode::Cr => Ok(LoadMode::ConstantResistance(self.cr_resistance()?)),
            Mode::CcCv => Ok(LoadMode::CcCv {
                current: self.cccv_current()?,
                voltage: self.cccv_voltage()?,
            }),
            Mode::CrCv => Ok(LoadMode::CrCv {
                resistance: self.crcv_resistance()?,
                voltage: self.crcv_voltage()?,
            }),
            Mode::Led => Ok(LoadMode::Led {
                voltage: self.led_voltage()?,
                current: self.led_current()?,
                coefficient: self.led_coefficient()?,
            }),
            Mode::Short => Ok(LoadMode::ShortCircuit),
            m => Err(Error::Unsupported(format!("use special-mode configuration getter for {m}"))),
        }
    }
    choice!(voltage_range, set_voltage_range, "LOAD", "VRANGE", Range, "voltage range");
    choice!(current_range, set_current_range, "LOAD", "CRANGE", Range, "current range");
    scalar!(ovp, set_ovp, "VOLT", "VMAX", Ovp, "OVP voltage in V");
    scalar!(ocp, set_ocp, "CURR", "IMAX", Ocp, "OCP current in A");
    scalar!(opp, set_opp, "POWE", "PMAX", Opp, "OPP power in W");
    /// Read V-on and V-off, in volts.
    pub fn voltage_thresholds(&mut self) -> Result<VoltageThresholds> {
        let [on, off] = self.query_float_pair("VOLT", "ON", "OFF")?;
        Ok(VoltageThresholds { on, off })
    }
    /// Set V-on and V-off, validating both before any write. Does not enable input.
    /// Reads the actual voltage range; an I/O error can leave a partial configuration.
    pub fn set_voltage_thresholds(&mut self, thresholds: VoltageThresholds) -> Result<()> {
        thresholds.validate(self.model(), Range::High)?;
        let range = self.voltage_range()?;
        thresholds.validate(self.model(), range)?;
        self.write_parameter("VOLT", "ON", thresholds.on)?;
        self.write_parameter("VOLT", "OFF", thresholds.off)
    }
    scalar!(cc_current, set_cc_current, "CURR", "CC", Current, "CC current in A");
    scalar!(cv_voltage, set_cv_voltage, "VOLT", "CV", Voltage, "CV voltage in V");
    scalar!(cp_power, set_cp_power, "POWE", "CP", Power, "CP power in W");
    scalar!(cr_resistance, set_cr_resistance, "RESI", "CR", Resistance, "CR resistance in ohms");
    scalar!(cccv_current, set_cccv_current, "CURR", "CCCV", Current, "CCCV current in A");
    scalar!(cccv_voltage, set_cccv_voltage, "VOLT", "CCCV", Voltage, "CCCV voltage in V");
    scalar!(crcv_resistance, set_crcv_resistance, "RESI", "CRCV", Resistance, "CRCV resistance in ohms");
    scalar!(crcv_voltage, set_crcv_voltage, "VOLT", "CRCV", Voltage, "CRCV voltage in V");
    scalar!(led_current, set_led_current, "CURR", "LED", Current, "LED I0 in A");
    scalar!(led_voltage, set_led_voltage, "VOLT", "LED", Voltage, "LED V0 in V");
    scalar!(led_coefficient, set_led_coefficient, "LED", "COEF", Coefficient, "LED coefficient");
    /// Actual protection/regulation state, including OT, LRV, FAN, UN and FAIL.
    pub fn protection(&mut self) -> Result<Protection> {
        self.query_parameter("LOAD", "ABNO")?
            .parse()
    }
    /// All three configured protection limits.
    pub fn protection_limits(&mut self) -> Result<ProtectionLimits> {
        Ok(ProtectionLimits {
            voltage: self.ovp()?,
            current: self.ocp()?,
            power: self.opp()?,
        })
    }
    /// Set protection limits, validating all three before writing the first.
    pub fn set_protection_limits(&mut self, p: ProtectionLimits) -> Result<()> {
        p.validate(self.model())?;
        self.set_ovp(p.voltage)?;
        self.set_ocp(p.current)?;
        self.set_opp(p.power)
    }
    /// Snapshot of actual input, mode and protection status (three sequential queries).
    pub fn status(&mut self) -> Result<ChannelStatus> {
        Ok(ChannelStatus {
            input: self.input()?,
            mode: self.mode()?,
            protection: self.protection()?,
        })
    }
    /// Read trigger source.
    pub fn trigger_mode(&mut self) -> Result<TriggerMode> {
        self.query_parameter("LOAD", "TRIG")?
            .parse()
    }
    /// Configure trigger source. ALL is only documented for 5406/5407.
    pub fn set_trigger_mode(&mut self, mode: TriggerMode) -> Result<()> {
        if mode == TriggerMode::All && !matches!(self.model(), Model::Et5406APlus | Model::Et5407APlus) {
            return Err(Error::Unsupported("ALL trigger source on this model".into()));
        }
        self.write_parameter("LOAD", "TRIG", mode)
    }
    /// Generate a global *TRG event (the protocol does not address this per-channel).
    pub fn trigger(&mut self) -> Result<()> {
        self.device.trigger()
    }
    /// Measure voltage in volts.
    pub fn read_voltage(&mut self) -> Result<f64> {
        self.query_float("MEAS", "VOLTAGE")
    }
    /// Measure current in amperes.
    pub fn read_current(&mut self) -> Result<f64> {
        self.query_float("MEAS", "CURRENT")
    }
    /// Measure power in watts.
    pub fn read_power(&mut self) -> Result<f64> {
        self.query_float("MEAS", "POWER")
    }
    /// Measure resistance in ohms; explicit unavailable/overrange values return None.
    pub fn read_resistance(&mut self) -> Result<Option<f64>> {
        let s = self.query_parameter("MEAS", "RESISTANCE")?;
        match scpi::payload(&s)
            .to_ascii_uppercase()
            .as_str()
        {
            "---" | "OL" | "OVER" | "INF" | "+INF" => Ok(None),
            _ => scpi::number(&s).map(Some),
        }
    }
    /// Combined measurement in wire order I,V,P,R, exposed as named SI fields.
    pub fn measure_all(&mut self) -> Result<Measurements> {
        Measurements::parse(&self.query_parameter("MEAS", "ALL")?)
    }

    fn validate_activation(&mut self) -> Result<()> {
        let mode = self.mode()?;
        self.require_mode(mode)?;
        let vr = self.voltage_range()?;
        let cr = self.current_range()?;
        let p = self.protection_limits()?;
        p.validate_activation(self.model(), [vr, cr])?;
        self.voltage_thresholds()?
            .validate(self.model(), vr)?;
        if self.protection()? != Protection::None {
            return Err(Error::Invalid("active protection/regulation fault prevents activation".into()));
        }
        let values = self.active_values(mode)?;
        for (kind, v) in values {
            p.validate_setpoint(self.model(), [vr, cr], kind, v)?;
        }
        // Ensure the operating mode did not change while reading its registers.
        if self.mode()? != mode {
            return Err(Error::Invalid("mode changed during activation preflight".into()));
        }
        Ok(())
    }
    fn active_values(&mut self, mode: Mode) -> Result<Vec<(Kind, f64)>> {
        match mode {
            Mode::Battery => {
                let c = self.battery_config()?;
                self.validate_battery(c)?;
                let kind = if c.submode == BatterySubmode::Cc {
                    Kind::Current
                } else {
                    Kind::BatteryResistance
                };
                Ok(match c.value {
                    BatteryValues::Single(v) => vec![(kind, v)],
                    BatteryValues::Stages(v) => v
                        .into_iter()
                        .map(|x| (kind, x))
                        .collect(),
                })
            }
            Mode::Transient => {
                let c = self.transient_config()?;
                self.validate_transient(c)?;
                let kind = if c.submode == TransientSubmode::Cc { Kind::Current } else { Kind::Voltage };
                Ok(c.values
                    .into_iter()
                    .map(|v| (kind, v))
                    .collect())
            }
            Mode::List => {
                let steps = self.list_steps()?;
                let rows = self.list_rows_range(1, steps)?;
                let mut values = Vec::new();
                for r in rows {
                    let ignored = matches!(r.mode, ListMode::Open | ListMode::Short);
                    let row = ListRow {
                        number: r.number,
                        mode: r.mode,
                        value: if ignored {
                            r.value.unwrap_or(0.)
                        } else {
                            r.value
                                .ok_or_else(|| Error::Protocol("missing LIST setpoint".into()))?
                        },
                        delay_s: r.delay_s,
                        comparison: r.comparison,
                        max: if r.comparison == Comparison::Off {
                            r.max.unwrap_or(0.)
                        } else {
                            r.max
                                .ok_or_else(|| Error::Protocol("missing LIST max".into()))?
                        },
                        min: if r.comparison == Comparison::Off {
                            r.min.unwrap_or(0.)
                        } else {
                            r.min
                                .ok_or_else(|| Error::Protocol("missing LIST min".into()))?
                        },
                    };
                    self.validate_list_row(row)?;
                    if !ignored {
                        values.push((Self::list_kind(r.mode), row.value));
                    }
                }
                Ok(values)
            }
            Mode::Scan => {
                let c = self.scan_config()?;
                self.validate_scan(c)?;
                Ok(c.start_end
                    .into_iter()
                    .map(|v| (Self::scan_kind(c.submode), v))
                    .collect())
            }
            _ => Ok(self.load_mode()?.setpoint_values()),
        }
    }
}

impl<T: Transport> Channel<'_, T> {
    /// Whether the qualification overlay is enabled.
    pub fn qualification_enabled(&mut self) -> Result<bool> {
        Ok(self
            .query_parameter("QUAL", "TEST")?
            .parse::<InputState>()?
            == InputState::On)
    }
    /// Toggle the qualification overlay; this is not the load input switch.
    pub fn set_qualification_enabled(&mut self, enabled: bool) -> Result<()> {
        self.write_parameter("QUAL", "TEST", if enabled { "ON" } else { "OFF" })
    }
    /// Last qualification comparison result.
    pub fn qualification_result(&mut self) -> Result<TestResult> {
        self.query_parameter("QUAL", "OUT")?
            .parse()
    }
    /// Qualification voltage bounds in V.
    pub fn qualification_voltage(&mut self) -> Result<[f64; 2]> {
        self.query_float_pair("QUAL", "VLOW", "VHIGH")
    }
    /// Set qualification voltage bounds.
    pub fn set_qualification_voltage(&mut self, v: [f64; 2]) -> Result<()> {
        self.set_pair("QUAL", "VLOW", "VHIGH", v, Kind::Voltage, true)
    }
    /// Qualification current bounds in A.
    pub fn qualification_current(&mut self) -> Result<[f64; 2]> {
        self.query_float_pair("QUAL", "CLOW", "CHIGH")
    }
    /// Set qualification current bounds.
    pub fn set_qualification_current(&mut self, v: [f64; 2]) -> Result<()> {
        self.set_pair("QUAL", "CLOW", "CHIGH", v, Kind::Current, true)
    }
    /// Qualification power bounds in W.
    pub fn qualification_power(&mut self) -> Result<[f64; 2]> {
        self.query_float_pair("QUAL", "PLOW", "PHIGH")
    }
    /// Set qualification power bounds.
    pub fn set_qualification_power(&mut self, v: [f64; 2]) -> Result<()> {
        self.set_pair("QUAL", "PLOW", "PHIGH", v, Kind::Power, true)
    }
    /// Configure qualification bounds and enable the overlay, preserving regulation mode.
    pub fn configure_qualification(&mut self, c: QualificationConfig) -> Result<()> {
        self.validate_pair(c.voltage, Kind::Voltage, true)?;
        self.validate_pair(c.current, Kind::Current, true)?;
        self.validate_pair(c.power, Kind::Power, true)?;
        self.set_qualification_voltage(c.voltage)?;
        self.set_qualification_current(c.current)?;
        self.set_qualification_power(c.power)?;
        self.set_qualification_enabled(true)
    }
    /// Read qualification bounds (the enable and outcome states have separate getters).
    pub fn qualification_config(&mut self) -> Result<QualificationConfig> {
        Ok(QualificationConfig {
            voltage: self.qualification_voltage()?,
            current: self.qualification_current()?,
            power: self.qualification_power()?,
        })
    }
}
