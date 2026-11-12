//! LIST step configuration, parameter readback and results.
use super::{Channel, Kind};
use crate::scpi::bounded;
use crate::{Comparison, Error, InputState, ListMode, ListResult, ListRow, ListRowReadback, ListStepMode, Mode, Result, Transport, modes};

impl<T: Transport> Channel<'_, T> {
    choice!(list_step_mode, set_list_step_mode, "LIST", "MODE", ListStepMode, "LIST step source");
    /// Whether LIST automatically repeats.
    pub fn list_loop(&mut self) -> Result<bool> {
        Ok(self
            .query_parameter("LIST", "LOOP")?
            .parse::<InputState>()?
            == InputState::On)
    }
    /// Enable or disable LIST repetition (not the input switch).
    pub fn set_list_loop(&mut self, enabled: bool) -> Result<()> {
        self.write_parameter("LIST", "LOOP", if enabled { "ON" } else { "OFF" })
    }
    /// Number of LIST steps to execute, 1..=10.
    pub fn list_steps(&mut self) -> Result<u8> {
        modes::row_number(&self.query_parameter("LIST", "NUM")?)
    }
    /// Set number of active LIST steps.
    pub fn set_list_steps(&mut self, steps: u8) -> Result<()> {
        bounded("LIST steps", steps as f64, 1., 10.)?;
        self.write_parameter("LIST", "NUM", steps)
    }
    pub(super) fn list_kind(mode: ListMode) -> Kind {
        match mode {
            ListMode::Cc | ListMode::Short => Kind::Current,
            ListMode::Cv | ListMode::Open => Kind::Voltage,
            ListMode::Cp => Kind::Power,
            ListMode::Cr => Kind::Resistance,
        }
    }
    pub(super) fn validate_list_row(&self, r: ListRow) -> Result<()> {
        bounded("LIST row", r.number as f64, 1., 10.)?;
        bounded("LIST delay [s]", r.delay_s as f64, 1., 60000.)?;
        if matches!(r.mode, ListMode::Open | ListMode::Short) {
            let limits = self.model().limits();
            let max = if r.mode == ListMode::Open { limits.voltage } else { limits.current };
            bounded("LIST OPEN/SHORT value", r.value, 0., max)?;
        } else {
            self.validate(r.value, Self::list_kind(r.mode))?;
        }
        // Comparisons are quantities independent of the row's regulation mode.
        let kind = match r.comparison {
            Comparison::Current => Some(Kind::Current),
            Comparison::Voltage => Some(Kind::VoltageComparison),
            Comparison::Power => Some(Kind::Power),
            Comparison::Resistance => Some(Kind::Resistance),
            Comparison::Off => None,
        };
        if let Some(kind) = kind {
            self.validate_pair([r.min, r.max], kind, true)?;
        } else {
            bounded("LIST min", r.min, 0., f64::MAX)?;
            bounded("LIST max", r.max, r.min, f64::MAX)?;
        }
        Ok(())
    }
    /// Write a single validated LIST row, with the original numeric enum encoding.
    pub fn set_list_row(&mut self, r: ListRow) -> Result<()> {
        self.validate_list_row(r)?;
        self.write_parameter(
            "LIST",
            "PARA",
            format!(
                "{},{},{},{},{},{},{}",
                r.number,
                r.mode.code(),
                r.value,
                r.delay_s,
                r.comparison.code(),
                r.max,
                r.min
            ),
        )
    }
    fn validate_list_rows(&self, rows: &[ListRow]) -> Result<()> {
        if rows.len() > 10 {
            return Err(Error::Invalid("at most ten LIST rows".into()));
        }
        let mut used = [false; 10];
        for &r in rows {
            self.validate_list_row(r)?;
            if std::mem::replace(&mut used[r.number as usize - 1], true) {
                return Err(Error::Invalid("duplicate LIST row".into()));
            }
        }
        Ok(())
    }
    /// Write rows after validating the entire batch; empty leaves all rows unchanged.
    pub fn set_list_rows(&mut self, rows: &[ListRow]) -> Result<()> {
        self.validate_list_rows(rows)?;
        for &r in rows {
            self.set_list_row(r)?;
        }
        Ok(())
    }
    /// Read all ten stored LIST parameter rows, preserving unavailable fields as None.
    pub fn list_rows(&mut self) -> Result<Vec<ListRowReadback>> {
        self.list_rows_range(1, 10)
    }
    /// Read a contiguous range timeout of 2.5 s per line.
    pub fn list_rows_range(&mut self, start: u8, count: u8) -> Result<Vec<ListRowReadback>> {
        if start == 0 || count == 0 || u16::from(start) + u16::from(count) > 11 {
            return Err(Error::Invalid("LIST range must fit 1..=10".into()));
        }
        let command = format!("{}? {start},{count}", self.build_channel_command("LIST", "PARA"));
        let lines = self
            .device
            .query_rows(&command, count as usize, self.device.session.config.list_timeout)?;
        let rows: Vec<_> = lines
            .iter()
            .map(|s| ListRowReadback::parse(s))
            .collect::<Result<_>>()?;
        for (offset, row) in rows.iter().enumerate() {
            if row.number as usize != start as usize + offset {
                return Err(Error::Protocol("out-of-order LIST rows".into()));
            }
        }
        Ok(rows)
    }
    /// Configure LIST step source and supplied rows, then select LIST mode.
    /// Does not change NUM or LOOP.
    pub fn configure_list(&mut self, step_mode: ListStepMode, rows: &[ListRow]) -> Result<()> {
        // Validate every row/duplicate before writing even the step source.
        self.validate_list_rows(rows)?;
        self.set_list_step_mode(step_mode)?;
        for &r in rows {
            self.set_list_row(r)?;
        }
        self.select_mode(Mode::List)
    }
    /// Retrieve final per-step LIST outcomes using the actual NUM property.
    pub fn list_results(&mut self) -> Result<Vec<ListResult>> {
        let steps = self.list_steps()?;
        let command = format!("{}? 1,{steps}", self.build_channel_command("LIST", "OUT"));
        let rows: Vec<_> = self
            .device
            .query_rows(&command, steps as usize, self.device.session.config.list_timeout)?
            .iter()
            .map(|s| ListResult::parse(s))
            .collect::<Result<_>>()?;
        for (i, row) in rows.iter().enumerate() {
            if row.number as usize != i + 1 {
                return Err(Error::Protocol("out-of-order LIST results".into()));
            }
        }
        Ok(rows)
    }
}
