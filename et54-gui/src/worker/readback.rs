//! Device reads; configuration is committed only after a complete successful readback.
use super::{BatteryReadback, BatterySample, BatteryState, ControlState, Sample, Snapshot, state::invalidate_settings};
use crate::i18n::Text;
use et54::{Channel, ChannelStatus, Error, Et54, InputState, Mode, Result, Transport};
use std::time::Instant;

pub(super) fn refresh_channel_settings<T: Transport>(device: &mut Et54<T>, state: &mut Snapshot) -> Result<()> {
    let mut ch = device.channel(state.selected)?;
    let status = ch.status()?;
    let settings = match status.mode {
        Mode::Battery | Mode::Transient | Mode::List | Mode::Scan => None,
        _ => Some(ch.load_mode()?),
    };
    if settings.is_some_and(|settings| settings.mode() != status.mode) {
        return Err(Error::Protocol(
            Text::ReadbackModeChanged
                .english()
                .into(),
        ));
    }
    let basic_setpoints = [ch.cc_current()?, ch.cv_voltage()?, ch.cp_power()?, ch.cr_resistance()?];
    let protections = ch.protection_limits()?;
    let ranges = [ch.voltage_range()?, ch.current_range()?];
    let voltage_thresholds = ch.voltage_thresholds()?;
    let battery = if status.mode == Mode::Battery {
        let readback = BatteryReadback {
            config: ch.battery_config()?,
            stages: ch.battery_cutoff_level()?,
        };
        let counters = read_battery_counters(&mut ch, state)?;
        if ch.mode()? != Mode::Battery {
            return Err(Error::Protocol(
                Text::BatteryModeChanged
                    .english()
                    .into(),
            ));
        }
        Some((readback, counters))
    } else {
        None
    };
    // Commit a complete readback together; never present a partially read configuration.
    if state.control != ControlState::Remote {
        state.sample = None;
    }
    state.status = Some(status);
    state.settings = settings;
    state.battery = battery.map(|(readback, _)| readback);
    if let Some((_, counters)) = battery {
        state.battery_sample = Some(counters);
    }
    update_battery_state(state, status);
    state.basic_setpoints = Some(basic_setpoints);
    state.protections = Some(protections);
    state.ranges = Some(ranges);
    state.voltage_thresholds = Some(voltage_thresholds);
    state.control = ControlState::Remote;
    state.settings_revision = state.settings_revision.wrapping_add(1);
    Ok(())
}
fn read_battery_counters<T: Transport>(ch: &mut Channel<'_, T>, state: &Snapshot) -> Result<BatterySample> {
    let capacity_ah = ch.battery_capacity()?;
    let energy_wh = ch.battery_energy()?;
    if capacity_ah < 0. || energy_wh < 0. {
        return Err(Error::Protocol(Text::NegativeCounters.english().into()));
    }
    Ok(BatterySample {
        capacity_ah,
        energy_wh,
        when: Instant::now(),
        channel: state.selected,
        epoch: state.epoch,
    })
}
fn update_battery_state(state: &mut Snapshot, status: ChannelStatus) {
    state.battery_state = if status.mode != Mode::Battery {
        BatteryState::Unknown
    } else if status.input == InputState::On {
        BatteryState::Active
    } else {
        match state.battery_state {
            BatteryState::Active => BatteryState::Ended,
            BatteryState::Ended | BatteryState::Stopped => state.battery_state,
            _ => BatteryState::Ready,
        }
    };
}
pub(super) fn poll_channel_measurements<T: Transport>(device: &mut Et54<T>, state: &mut Snapshot, sequence: &mut u64) -> Result<()> {
    let mut ch = device.channel(state.selected)?;
    let values = ch.measure_all()?;
    let status = ch.status()?;
    let mode_changed = state
        .status
        .is_some_and(|old| old.mode != status.mode)
        || state
            .settings
            .is_some_and(|settings| settings.mode() != status.mode);
    if mode_changed {
        invalidate_settings(state);
        state.message = Text::ModeChanged.into();
    }
    if status.mode == Mode::Battery {
        let counters = read_battery_counters(&mut ch, state)?;
        state.battery_sample = Some(counters);
    }
    update_battery_state(state, status);
    if !mode_changed && status.mode == Mode::Battery {
        state.message = state.battery_state.label().into();
    }
    *sequence = sequence.wrapping_add(1);
    state.sample = Some(Sample {
        values,
        when: Instant::now(),
        sequence: *sequence,
        channel: state.selected,
        epoch: state.epoch,
    });
    state.status = Some(status);
    Ok(())
}
