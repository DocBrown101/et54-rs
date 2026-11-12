//! Transient A/B configuration and readback.
use super::{Channel, Kind};
use crate::scpi;
use crate::{Mode, Result, TransientConfig, TransientSubmode, TransientTrigger, Transport};

impl<T: Transport> Channel<'_, T> {
    choice!(transient_submode, set_transient_submode, "TRAN", "STATE", TransientSubmode, "transient submode");
    choice!(
        transient_trigger,
        set_transient_trigger,
        "TRAN",
        "MODE",
        TransientTrigger,
        "transient trigger mode (COUT = continuous)"
    );
    /// Read transient A/B current in A.
    pub fn transient_current(&mut self) -> Result<[f64; 2]> {
        self.query_float_pair("CURR", "TA", "TB")
    }
    /// Set transient A/B current in A, without sorting their order.
    pub fn set_transient_current(&mut self, v: [f64; 2]) -> Result<()> {
        self.set_pair("CURR", "TA", "TB", v, Kind::Current, false)
    }
    /// Read transient A/B voltage in V.
    pub fn transient_voltage(&mut self) -> Result<[f64; 2]> {
        self.query_float_pair("VOLT", "TA", "TB")
    }
    /// Set transient A/B voltage in V.
    pub fn set_transient_voltage(&mut self, v: [f64; 2]) -> Result<()> {
        self.set_pair("VOLT", "TA", "TB", v, Kind::Voltage, false)
    }
    /// Read transient pulse widths in **milliseconds**.
    pub fn transient_width_ms(&mut self) -> Result<[u32; 2]> {
        Ok([
            scpi::integer(&self.query_parameter("TIME", "WA")?)?,
            scpi::integer(&self.query_parameter("TIME", "WB")?)?,
        ])
    }
    /// Set transient pulse widths in milliseconds, minimum 50 ms.
    pub fn set_transient_width_ms(&mut self, v: [u32; 2]) -> Result<()> {
        self.set_pair("TIME", "WA", "WB", v.map(f64::from), Kind::Width, false)
    }
    pub(super) fn validate_transient(&self, c: TransientConfig) -> Result<()> {
        self.validate_pair(c.values, if c.submode == TransientSubmode::Cc { Kind::Current } else { Kind::Voltage }, false)?;
        self.validate_pair(c.width_ms.map(f64::from), Kind::Width, false)
    }
    /// Set all transient parameters before selecting TRAN; never activates the load.
    pub fn configure_transient(&mut self, c: TransientConfig) -> Result<()> {
        self.validate_transient(c)?;
        self.set_transient_submode(c.submode)?;
        self.set_transient_trigger(c.trigger)?;
        match c.submode {
            TransientSubmode::Cc => self.set_transient_current(c.values)?,
            TransientSubmode::Cv => self.set_transient_voltage(c.values)?,
        }
        self.set_transient_width_ms(c.width_ms)?;
        self.select_mode(Mode::Transient)
    }
    /// Read the complete actual transient configuration.
    pub fn transient_config(&mut self) -> Result<TransientConfig> {
        let submode = self.transient_submode()?;
        let trigger = self.transient_trigger()?;
        let values = match submode {
            TransientSubmode::Cc => self.transient_current()?,
            TransientSubmode::Cv => self.transient_voltage()?,
        };
        Ok(TransientConfig {
            submode,
            trigger,
            values,
            width_ms: self.transient_width_ms()?,
        })
    }
}
