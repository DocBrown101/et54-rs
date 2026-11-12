//! SCAN configuration and readback.
use super::{Channel, Kind};
use crate::scpi;
use crate::{Error, Mode, Result, ScanComparison, ScanConfig, ScanSubmode, ScanThreshold, Transport};

impl<T: Transport> Channel<'_, T> {
    choice!(scan_submode, set_scan_submode, "SCAN", "TYPE", ScanSubmode, "SCAN submode");
    choice!(scan_threshold, set_scan_threshold, "SCAN", "THTYPE", ScanThreshold, "SCAN threshold selector");
    choice!(scan_compare, set_scan_compare, "SCAN", "COMPARE", ScanComparison, "SCAN comparison selector");
    /// VTH/VMIN threshold in volts. DROP returns Unsupported instead of invented data.
    pub fn scan_threshold_value(&mut self) -> Result<f64> {
        let parameter = match self.scan_threshold()? {
            ScanThreshold::Voltage => "VTH",
            ScanThreshold::MinimumVoltage => "VMIN",
            ScanThreshold::Drop => {
                return Err(Error::Unsupported("SCAN DROP numeric threshold has no documented command".into()));
            }
        };
        self.query_float("VOLT", parameter)
    }
    /// Set VTH/VMIN voltage. DROP returns Unsupported before any write.
    pub fn set_scan_threshold_value(&mut self, value: f64) -> Result<()> {
        self.validate(value, Kind::Voltage)?;
        let parameter = match self.scan_threshold()? {
            ScanThreshold::Voltage => "VTH",
            ScanThreshold::MinimumVoltage => "VMIN",
            ScanThreshold::Drop => {
                return Err(Error::Unsupported("SCAN DROP numeric threshold has no documented command".into()));
            }
        };
        self.write_parameter("VOLT", parameter, value)
    }
    fn scan_system(mode: ScanSubmode) -> &'static str {
        match mode {
            ScanSubmode::Cc => "CURR",
            ScanSubmode::Cv => "VOLT",
            ScanSubmode::Cp => "POWE",
        }
    }
    pub(super) fn scan_kind(mode: ScanSubmode) -> Kind {
        match mode {
            ScanSubmode::Cc => Kind::Current,
            ScanSubmode::Cv => Kind::Voltage,
            ScanSubmode::Cp => Kind::Power,
        }
    }
    /// SCAN lower/upper bounds.
    pub fn scan_limits(&mut self) -> Result<[f64; 2]> {
        let m = self.scan_submode()?;
        self.query_float_pair(Self::scan_system(m), "LOW", "HIGH")
    }
    /// Set SCAN comparison bounds in submode units.
    pub fn set_scan_limits(&mut self, v: [f64; 2]) -> Result<()> {
        let m = self.scan_submode()?;
        self.set_pair(Self::scan_system(m), "LOW", "HIGH", v, Self::scan_kind(m), true)
    }
    /// SCAN start/end setpoints in submode units.
    pub fn scan_start_end(&mut self) -> Result<[f64; 2]> {
        let m = self.scan_submode()?;
        self.query_float_pair(Self::scan_system(m), "START", "END")
    }
    /// Set ascending SCAN start/end setpoints.
    pub fn set_scan_start_end(&mut self, v: [f64; 2]) -> Result<()> {
        let m = self.scan_submode()?;
        self.set_pair(Self::scan_system(m), "START", "END", v, Self::scan_kind(m), true)
    }
    /// SCAN increment in submode units.
    pub fn scan_step(&mut self) -> Result<f64> {
        let m = self.scan_submode()?;
        self.query_float(Self::scan_system(m), "STEP")
    }
    /// Set a positive SCAN increment.
    pub fn set_scan_step(&mut self, value: f64) -> Result<()> {
        let m = self.scan_submode()?;
        if value <= 0. {
            return Err(Error::Invalid("positive SCAN step required".into()));
        }
        self.set_float(Self::scan_system(m), "STEP", value, Self::scan_kind(m))
    }
    /// SCAN dwell time in seconds.
    pub fn scan_step_delay(&mut self) -> Result<u32> {
        scpi::integer(&self.query_parameter("TIME", "STEP")?)
    }
    /// Set SCAN dwell time, integer seconds.
    pub fn set_scan_step_delay(&mut self, seconds: u32) -> Result<()> {
        self.set_float("TIME", "STEP", seconds as f64, Kind::StepDelay)
    }
    pub(super) fn validate_scan(&self, c: ScanConfig) -> Result<()> {
        match (c.threshold, c.threshold_value) {
            (ScanThreshold::Drop, Some(_)) => {
                return Err(Error::Unsupported("SCAN DROP numeric threshold".into()));
            }
            (ScanThreshold::Drop, None) => {}
            (_, Some(v)) => self.validate(v, Kind::Voltage)?,
            (_, None) => return Err(Error::Invalid("VTH/VMIN requires voltage threshold".into())),
        }
        self.validate_pair(c.limits, Self::scan_kind(c.submode), true)?;
        self.validate_pair(c.start_end, Self::scan_kind(c.submode), true)?;
        self.validate(c.step, Self::scan_kind(c.submode))?;
        if c.step <= 0. {
            return Err(Error::Invalid("positive SCAN step required".into()));
        }
        self.validate(c.step_delay_s as f64, Kind::StepDelay)
    }
    /// Configure every implemented SCAN parameter, then select SCAN. DROP is allowed
    /// only without a numeric threshold, since neither reference defines its command.
    pub fn configure_scan(&mut self, c: ScanConfig) -> Result<()> {
        self.validate_scan(c)?;
        self.set_scan_submode(c.submode)?;
        self.set_scan_threshold(c.threshold)?;
        if let Some(v) = c.threshold_value {
            self.set_scan_threshold_value(v)?;
        }
        self.set_scan_compare(c.comparison)?;
        self.set_scan_limits(c.limits)?;
        self.set_scan_start_end(c.start_end)?;
        self.set_scan_step(c.step)?;
        self.set_scan_step_delay(c.step_delay_s)?;
        self.select_mode(Mode::Scan)
    }
    /// Read every implemented SCAN setting. Does not invent an outcome query.
    pub fn scan_config(&mut self) -> Result<ScanConfig> {
        let submode = self.scan_submode()?;
        let threshold = self.scan_threshold()?;
        let threshold_value = if threshold == ScanThreshold::Drop {
            None
        } else {
            Some(self.scan_threshold_value()?)
        };
        Ok(ScanConfig {
            submode,
            threshold,
            threshold_value,
            comparison: self.scan_compare()?,
            limits: self.scan_limits()?,
            start_end: self.scan_start_end()?,
            step: self.scan_step()?,
            step_delay_s: self.scan_step_delay()?,
        })
    }
}
