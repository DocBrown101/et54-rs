//! Synchronous ET54 driver, independent of any GUI or async runtime.
//!
//! Connecting only identifies the device. No reset or input activation is sent.
//! A [`Channel`] exclusively borrows the device, serializing all SCPI operations.
//! Hardware input state after an I/O failure is **unknown**, never implicitly OFF.
//!
//! ```no_run
//! use et54::Et54;
//! # fn main() -> et54::Result<()> {
//! let mut device = Et54::open("COM3")?;
//! println!("{:?}", device.info());
//! println!("{:?}", device.channel(1)?.measure_all()?);
//! device.close()?;
//! # Ok(())
//! # }
//! ```
#![warn(missing_docs)]

pub mod channel;
pub mod device;
pub mod error;
pub mod measurement;
pub mod models;
pub mod modes;
pub mod scpi;
pub mod transport;

pub use channel::{Channel, ChannelStatus, ProtectionLimits, VoltageThresholds};
pub use device::{DeviceInfo, Et54};
pub use error::{Error, Result};
pub use measurement::Measurements;
pub use models::Model;
pub use modes::*;
pub use transport::{ConnectionConfig, MockTransport, SerialTransport, Transport, available_ports};
