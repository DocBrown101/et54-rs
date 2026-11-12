//! Worker commands, complete readbacks and their validity rules. No device I/O.
use crate::i18n::{Message, Text};
use et54::{
    BatteryConfig, BatteryCutoff, BatterySubmode, BatteryValues, ChannelStatus, ConnectionConfig, DeviceInfo, Error, LoadMode, Measurements, Mode, Model,
    ProtectionLimits, Range, Result, VoltageThresholds,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub values: Measurements,
    pub when: Instant,
    pub sequence: u64,
    pub channel: u8,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatterySettings {
    pub current: f64,
    pub cutoff: f64,
}
impl BatterySettings {
    pub fn to_battery_config(self) -> BatteryConfig {
        BatteryConfig {
            submode: BatterySubmode::Cc,
            value: BatteryValues::Stages([self.current; 3]),
            cutoff: BatteryCutoff::Voltage,
            cutoff_value: BatteryValues::Stages([self.cutoff; 3]),
        }
    }
    pub fn validate(self, model: Model, protections: ProtectionLimits, ranges: [Range; 2], thresholds: VoltageThresholds) -> Result<()> {
        validate_gui_settings(model, LoadMode::ConstantCurrent(self.current), protections, ranges, thresholds)?;
        LoadMode::ConstantVoltage(self.cutoff).validate(model, protections, ranges)?;
        if self.current <= 0. {
            return Err(Error::Invalid(Text::PositiveCurrent.english().into()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatteryReadback {
    pub config: BatteryConfig,
    pub stages: u8,
}
impl BatteryReadback {
    pub fn single_stage_cc_settings(self) -> Option<BatterySettings> {
        match self.config {
            BatteryConfig {
                submode: BatterySubmode::Cc,
                cutoff: BatteryCutoff::Voltage,
                value: BatteryValues::Stages(currents),
                cutoff_value: BatteryValues::Stages(voltages),
            } if self.stages == 1 => Some(BatterySettings {
                current: currents[0],
                cutoff: voltages[0],
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BatterySample {
    pub capacity_ah: f64,
    pub energy_wh: f64,
    pub when: Instant,
    pub channel: u8,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryState {
    Unknown,
    Ready,
    Active,
    Ended,
    Stopped,
}
impl BatteryState {
    pub fn label(self) -> Text {
        match self {
            Self::Unknown => Text::BatteryUnknown,
            Self::Ready => Text::BatteryReady,
            Self::Active => Text::BatteryActive,
            Self::Ended => Text::BatteryEnded,
            Self::Stopped => Text::BatteryStopped,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub ports: Vec<String>,
    pub last_connected_port: Option<String>,
    pub info: Option<DeviceInfo>,
    pub connected: bool,
    pub control: ControlState,
    pub busy: bool,
    pub selected: u8,
    pub status: Option<ChannelStatus>,
    pub settings: Option<LoadMode>,
    pub battery: Option<BatteryReadback>,
    pub battery_sample: Option<BatterySample>,
    pub battery_state: BatteryState,
    pub basic_setpoints: Option<[f64; 4]>,
    pub protections: Option<ProtectionLimits>,
    pub ranges: Option<[Range; 2]>,
    pub voltage_thresholds: Option<VoltageThresholds>,
    pub sample: Option<Sample>,
    pub error: Option<Message>,
    pub message: Message,
    pub completed: u64,
    pub settings_revision: u64,
    pub epoch: u64,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            last_connected_port: None,
            info: None,
            connected: false,
            control: ControlState::Unknown,
            busy: false,
            selected: 1,
            status: None,
            settings: None,
            battery: None,
            battery_sample: None,
            battery_state: BatteryState::Unknown,
            basic_setpoints: None,
            protections: None,
            ranges: None,
            voltage_thresholds: None,
            sample: None,
            error: None,
            message: Text::NotConnectedUnknown.into(),
            completed: 0,
            settings_revision: 0,
            epoch: 0,
        }
    }
}
impl Snapshot {
    pub fn has_current_settings(&self) -> bool {
        self.status.is_some_and(|status| {
            if status.mode == Mode::Battery {
                self.battery
                    .and_then(BatteryReadback::single_stage_cc_settings)
                    .is_some()
            } else {
                self.settings
                    .is_some_and(|settings| settings.mode() == status.mode)
            }
        }) && self.basic_setpoints.is_some()
            && self.protections.is_some()
            && self.ranges.is_some()
            && self.voltage_thresholds.is_some()
    }
}

/// Communication-derived state; the protocol has no direct keyboard-lock query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlState {
    Unknown,
    Remote,
    Local,
}

#[derive(Debug)]
pub enum Command {
    Ports,
    Connect(String, ConnectionConfig),
    Disconnect,
    Select(u8),
    Interval(Duration),
    Configure {
        mode: LoadMode,
        protections: ProtectionLimits,
        ranges: [Range; 2],
        voltage_thresholds: VoltageThresholds,
    },
    ConfigureBattery {
        battery: BatterySettings,
        protections: ProtectionLimits,
        ranges: [Range; 2],
        voltage_thresholds: VoltageThresholds,
    },
    Enable,
    Disable,
    Refresh,
    Local,
    Remote,
}

pub(super) fn invalidate_settings(state: &mut Snapshot) {
    state.settings = None;
    state.battery = None;
    state.battery_state = BatteryState::Unknown;
    state.basic_setpoints = None;
    state.protections = None;
    state.ranges = None;
    state.voltage_thresholds = None;
    state.settings_revision = state.settings_revision.wrapping_add(1);
}
pub(super) fn clear_device(state: &mut Snapshot) {
    state.connected = false;
    state.control = ControlState::Unknown;
    state.info = None;
    state.status = None;
    invalidate_settings(state);
    state.sample = None;
    state.epoch = state.epoch.wrapping_add(1);
}

pub fn validate_gui_settings(model: Model, mode: LoadMode, p: ProtectionLimits, ranges: [Range; 2], voltage_thresholds: VoltageThresholds) -> Result<()> {
    if !matches!(
        mode,
        LoadMode::ConstantCurrent(_) | LoadMode::ConstantVoltage(_) | LoadMode::ConstantPower(_) | LoadMode::ConstantResistance(_)
    ) {
        return Err(Error::Unsupported(Text::BasicModesOnly.english().into()));
    }
    mode.validate(model, p, ranges)?;
    voltage_thresholds.validate(model, ranges[0])
}
