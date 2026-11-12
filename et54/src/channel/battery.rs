//! Battery discharge configuration and readback.
use super::{Channel, Kind};
use crate::scpi::{self, bounded};
use crate::{BatteryConfig, BatteryCutoff, BatterySubmode, BatteryValues, Error, Mode, Result, Transport};

impl<T: Transport> Channel<'_, T> {
    choice!(
        battery_submode,
        set_battery_submode,
        "BATT",
        "MODE",
        BatterySubmode,
        "battery discharge submode"
    );
    /// Battery cutoff selector; supports both full-word and single-letter responses.
    pub fn battery_cutoff(&mut self) -> Result<BatteryCutoff> {
        self.query_parameter("BATT", "BCUT")?
            .parse()
    }
    /// Select the battery cutoff criterion, using the single-letter write form.
    pub fn set_battery_cutoff(&mut self, value: BatteryCutoff) -> Result<()> {
        self.write_parameter("BATT", "BCUT", value.as_str())
    }
    /// CC battery current in A, scalar or three stages according to actual cutoff.
    pub fn battery_current(&mut self) -> Result<BatteryValues> {
        if self.battery_cutoff()? == BatteryCutoff::Voltage {
            Ok(BatteryValues::Stages([
                self.query_float("CURR", "BCC1")?,
                self.query_float("CURR", "BCC2")?,
                self.query_float("CURR", "BCC3")?,
            ]))
        } else {
            Ok(BatteryValues::Single(self.query_float("CURR", "BCC")?))
        }
    }
    /// Set CC battery current(s); scalar voltage-cutoff currents repeat to three stages.
    pub fn set_battery_current(&mut self, value: BatteryValues) -> Result<()> {
        let voltage = self.battery_cutoff()? == BatteryCutoff::Voltage;
        if voltage {
            for v in value.stages() {
                self.validate(v, Kind::Current)?;
            }
            for (i, v) in value.stages().into_iter().enumerate() {
                self.write_parameter("CURR", &format!("BCC{}", i + 1), v)?;
            }
            Ok(())
        } else {
            self.set_float("CURR", "BCC", value.single()?, Kind::Current)
        }
    }
    scalar!(
        battery_resistance,
        set_battery_resistance,
        "RESI",
        "BCR",
        BatteryResistance,
        "battery CR resistance in ohms"
    );
    /// Read cutoff V / s / Ah / Wh according to actual submode and cutoff selector.
    pub fn battery_cutoff_value(&mut self) -> Result<BatteryValues> {
        let value = match self.battery_cutoff()? {
            BatteryCutoff::Voltage => {
                if self.battery_submode()? == BatterySubmode::Cc {
                    return Ok(BatteryValues::Stages([
                        self.query_float("VOLT", "BCC1")?,
                        self.query_float("VOLT", "BCC2")?,
                        self.query_float("VOLT", "BCC3")?,
                    ]));
                }
                self.query_float("VOLT", "BCR")?
            }
            BatteryCutoff::Time => self.query_float("TIME", "BTT")?,
            BatteryCutoff::Capacity => self.query_float("BATT", "BTC")?,
            BatteryCutoff::Energy => self.query_float("BATT", "BTE")?,
        };
        Ok(BatteryValues::Single(value))
    }
    /// Set the cutoff value using actual selector/submode. CR voltage cutoff uses
    /// VOLT:BCR, correcting the original's erroneous CURR:BCC getter/BCC1 setter.
    pub fn set_battery_cutoff_value(&mut self, value: BatteryValues) -> Result<()> {
        match self.battery_cutoff()? {
            BatteryCutoff::Voltage => {
                if self.battery_submode()? == BatterySubmode::Cc {
                    for v in value.stages() {
                        self.validate(v, Kind::Voltage)?;
                    }
                    for (i, v) in value.stages().into_iter().enumerate() {
                        self.write_parameter("VOLT", &format!("BCC{}", i + 1), v)?;
                    }
                    Ok(())
                } else {
                    self.set_float("VOLT", "BCR", value.single()?, Kind::Voltage)
                }
            }
            BatteryCutoff::Time => {
                let v = value.single()?;
                if v.fract() != 0. {
                    return Err(Error::Invalid("battery timer requires integer seconds".into()));
                }
                self.set_float("TIME", "BTT", v, Kind::Time)
            }
            BatteryCutoff::Capacity => self.set_float("BATT", "BTC", value.single()?, Kind::Capacity),
            BatteryCutoff::Energy => self.set_float("BATT", "BTE", value.single()?, Kind::Capacity),
        }
    }
    /// Discharged capacity in Ah (no implicit mAh conversion).
    pub fn battery_capacity(&mut self) -> Result<f64> {
        self.query_float("BATT", "CAPA")
    }
    /// Discharged energy in Wh.
    pub fn battery_energy(&mut self) -> Result<f64> {
        self.query_float("BATT", "ENER")
    }
    /// BAEN stage enable value, 1..=3. Physical interpretation is firmware-dependent.
    pub fn battery_cutoff_level(&mut self) -> Result<u8> {
        let v = scpi::integer(&self.query_parameter("BATT", "BAEN")?)?;
        if (1..=3).contains(&v) {
            Ok(v as u8)
        } else {
            Err(Error::Protocol(format!("BAEN {v}")))
        }
    }
    /// Set BAEN to 1..=3; physical interpretation is firmware-dependent.
    pub fn set_battery_cutoff_level(&mut self, level: u8) -> Result<()> {
        bounded("battery stage", level as f64, 1., 3.)?;
        self.write_parameter("BATT", "BAEN", level)
    }
    pub(super) fn validate_battery(&self, c: BatteryConfig) -> Result<()> {
        let staged = c.cutoff == BatteryCutoff::Voltage && c.submode == BatterySubmode::Cc;
        let currents = if staged { c.value.stages().to_vec() } else { vec![c.value.single()?] };
        let kind = if c.submode == BatterySubmode::Cc {
            Kind::Current
        } else {
            Kind::BatteryResistance
        };
        for v in currents {
            self.validate(v, kind)?;
        }
        let cutoffs = if staged {
            c.cutoff_value.stages().to_vec()
        } else {
            vec![c.cutoff_value.single()?]
        };
        for v in cutoffs {
            self.validate(
                v,
                match c.cutoff {
                    BatteryCutoff::Voltage => Kind::Voltage,
                    BatteryCutoff::Time => Kind::Time,
                    _ => Kind::Capacity,
                },
            )?;
            if c.cutoff == BatteryCutoff::Time && v.fract() != 0. {
                return Err(Error::Invalid("battery timer requires integer seconds".into()));
            }
        }
        Ok(())
    }
    /// Configure a complete CC/CR battery discharge, without enabling the input.
    /// Validation of every argument precedes all writes; I/O failure may leave partial settings.
    pub fn configure_battery(&mut self, c: BatteryConfig) -> Result<()> {
        self.validate_battery(c)?;
        self.set_battery_submode(c.submode)?;
        self.set_battery_cutoff(c.cutoff)?;
        match c.submode {
            BatterySubmode::Cc => self.set_battery_current(c.value)?,
            BatterySubmode::Cr => self.set_battery_resistance(c.value.single()?)?,
        }
        self.set_battery_cutoff_value(c.cutoff_value)?;
        self.select_mode(Mode::Battery)
    }
    /// Read all actual battery configuration registers used by the current selectors.
    pub fn battery_config(&mut self) -> Result<BatteryConfig> {
        let submode = self.battery_submode()?;
        let cutoff = self.battery_cutoff()?;
        let value = match submode {
            BatterySubmode::Cc => self.battery_current()?,
            BatterySubmode::Cr => BatteryValues::Single(self.battery_resistance()?),
        };
        Ok(BatteryConfig {
            submode,
            cutoff,
            value,
            cutoff_value: self.battery_cutoff_value()?,
        })
    }
}
