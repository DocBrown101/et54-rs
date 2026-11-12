//! Execute explicit commands; only errors after device access invalidate readbacks.
use super::{
    BatteryReadback, BatteryState, Command, ControlState, Snapshot,
    readback::refresh_channel_settings,
    state::{clear_device, invalidate_settings},
    validate_gui_settings,
};
use crate::i18n::{Message, Text};
use et54::{ConnectionConfig, Error, Et54, InputState, Mode, Result, Transport};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

pub(super) fn execute<T, F>(
    command: Command,
    device: &mut Option<Et54<T>>,
    state: &mut Snapshot,
    interval: &mut Duration,
    stop: &Arc<AtomicBool>,
    connect: &mut F,
) where
    T: Transport,
    F: FnMut(&str, ConnectionConfig, Arc<AtomicBool>) -> Result<Et54<T>>,
{
    let device_command = !matches!(command, Command::Ports | Command::Interval(_));
    let mut device_accessed = false;
    let result = (|| -> Result<()> {
        match command {
            Command::Ports => state.ports = et54::available_ports()?,
            Command::Connect(path, config) => {
                device_accessed = true;
                clear_device(state);
                state.battery_sample = None;
                if let Some(mut old) = device.take() {
                    old.close()?;
                }
                state.selected = 1;
                let mut new = connect(&path, config, stop.clone())?;
                refresh_channel_settings(&mut new, state)?;
                state.info = Some(new.info().clone());
                state.connected = true;
                *device = Some(new);
                state.last_connected_port = Some(path);
                state.message = Text::ConnectedRead.into();
            }
            Command::Disconnect => {
                device_accessed = true;
                let close = device
                    .take()
                    .map(|mut d| d.close())
                    .transpose();
                clear_device(state);
                state.message = Text::DisconnectedUnchanged.into();
                if close?.is_some() {
                    state.message = Text::DisconnectedUnlocked.into();
                }
            }
            Command::Interval(value) => {
                if !(Duration::from_millis(500)..=Duration::from_secs(60)).contains(&value) {
                    return Err(Error::Invalid(Text::PollBounds.english().into()));
                }
                *interval = value;
            }
            command => {
                if state.control == ControlState::Local && !matches!(command, Command::Local | Command::Remote | Command::Disable) {
                    return Err(Error::Invalid(Text::ResumeFirst.english().into()));
                }
                let d = device
                    .as_mut()
                    .ok_or(Error::Unavailable)?;
                match command {
                    Command::Local => {
                        device_accessed = true;
                        // Never query after LOCA: even read-only SCPI locks the keys again.
                        state.status = None;
                        invalidate_settings(state);
                        d.unlock()?;
                        state.control = ControlState::Local;
                        state.message = Text::LocalReleased.into();
                    }
                    Command::Remote => {
                        device_accessed = true;
                        refresh_channel_settings(d, state)?;
                        state.message = Text::RemoteResumed.into();
                    }
                    Command::Select(n) => {
                        d.channel(n)?;
                        state.selected = n;
                        state.sample = None;
                        state.battery_sample = None;
                        state.battery_state = BatteryState::Unknown;
                        device_accessed = true;
                        refresh_channel_settings(d, state)?;
                        state.message = Message::ChannelRead(n);
                    }
                    Command::Refresh => {
                        device_accessed = true;
                        refresh_channel_settings(d, state)?;
                        state.message = Text::SettingsRead.into();
                    }
                    Command::Configure {
                        mode,
                        protections,
                        ranges,
                        voltage_thresholds,
                    } => {
                        // Reject the full GUI configuration before the first hardware write.
                        validate_gui_settings(d.info().model, mode, protections, ranges, voltage_thresholds)?;
                        let mut ch = d.channel(state.selected)?;
                        // Changing mode/ranges/protection on a live input is deliberately explicit: first verified OFF.
                        device_accessed = true;
                        ch.disable()?;
                        ch.set_voltage_range(ranges[0])?;
                        ch.set_current_range(ranges[1])?;
                        ch.set_voltage_thresholds(voltage_thresholds)?;
                        ch.set_protection_limits(protections)?;
                        ch.set_mode(mode)?;
                        refresh_channel_settings(d, state)?;
                        state.message = Text::SettingsApplied.into();
                    }
                    Command::Enable => {
                        if !state.has_current_settings() {
                            return Err(Error::Invalid(Text::ReadBeforeOn.english().into()));
                        }
                        device_accessed = true;
                        let battery = state
                            .status
                            .is_some_and(|s| s.mode == Mode::Battery);
                        let expected = (state.settings, state.battery, state.protections, state.ranges, state.voltage_thresholds);
                        // Never activate a different profile than the one shown in the UI.
                        refresh_channel_settings(d, state)?;
                        if expected != (state.settings, state.battery, state.protections, state.ranges, state.voltage_thresholds) {
                            return Err(Error::Invalid(Text::SettingsChanged.english().into()));
                        }
                        if battery {
                            let config = state
                                .battery
                                .and_then(BatteryReadback::single_stage_cc_settings)
                                .ok_or_else(|| {
                                    Error::Unsupported(
                                        Text::SingleStageRequired
                                            .english()
                                            .into(),
                                    )
                                })?;
                            config.validate(
                                d.info().model,
                                state
                                    .protections
                                    .ok_or(Error::Unavailable)?,
                                state.ranges.ok_or(Error::Unavailable)?,
                                state
                                    .voltage_thresholds
                                    .ok_or(Error::Unavailable)?,
                            )?;
                            if state
                                .status
                                .is_none_or(|s| s.input != InputState::Off)
                            {
                                return Err(Error::Invalid(
                                    Text::BatteryOffRequired
                                        .english()
                                        .into(),
                                ));
                            }
                        }
                        d.channel(state.selected)?.enable()?;
                        if battery {
                            state.battery_state = BatteryState::Active;
                        }
                        refresh_channel_settings(d, state)?;
                        state.message = if battery { state.battery_state.label() } else { Text::OnConfirmed }.into();
                    }
                    Command::Disable => {
                        device_accessed = true;
                        d.channel(state.selected)?.disable()?;
                        refresh_channel_settings(d, state)?;
                        if state
                            .status
                            .is_some_and(|s| s.mode == Mode::Battery)
                        {
                            state.battery_state = BatteryState::Stopped;
                        }
                        state.message = Text::OffConfirmed.into();
                    }
                    Command::ConfigureBattery {
                        battery,
                        protections,
                        ranges,
                        voltage_thresholds,
                    } => {
                        battery.validate(d.info().model, protections, ranges, voltage_thresholds)?;
                        device_accessed = true;
                        let mut ch = d.channel(state.selected)?;
                        ch.disable()?;
                        ch.set_voltage_range(ranges[0])?;
                        ch.set_current_range(ranges[1])?;
                        ch.set_voltage_thresholds(voltage_thresholds)?;
                        ch.set_protection_limits(protections)?;
                        ch.configure_battery(battery.to_battery_config())?;
                        ch.set_battery_cutoff_level(1)?;
                        refresh_channel_settings(d, state)?;
                        if state
                            .battery
                            .and_then(BatteryReadback::single_stage_cc_settings)
                            .is_none()
                            || state
                                .status
                                .is_none_or(|s| s.input != InputState::Off)
                        {
                            return Err(Error::Protocol(
                                Text::BatteryNotConfirmed
                                    .english()
                                    .into(),
                            ));
                        }
                        state.battery_state = BatteryState::Ready;
                        state.message = Text::BatteryApplied.into();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        state.error = Some(Message::from(&e));
        if device_command {
            state.message = Text::CommandFailed.into();
        }
        if device_accessed {
            state.control = ControlState::Unknown;
            state.status = None;
            state.sample = None;
            invalidate_settings(state);
            state.message = Text::CommandFailedUnknown.into();
        }
        if matches!(e, Error::DisableFailed { .. })
            || device
                .as_ref()
                .is_some_and(|d| !d.is_available())
        {
            device.take();
            clear_device(state);
            state.message = Text::CommunicationFailed.into();
        }
    }
}
