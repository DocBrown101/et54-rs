//! Cancellation during device I/O and a final, bounded unlock when closing.
use et54::{ConnectionConfig, Result, Transport};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) struct Cancellable<T> {
    transport: T,
    stop: Arc<AtomicBool>,
    unlock_on_close: Option<Vec<u8>>,
    close_timeout: Duration,
}
impl<T: Transport> Cancellable<T> {
    pub(super) fn new(transport: T, stop: Arc<AtomicBool>, config: &ConnectionConfig) -> Self {
        let mut unlock = b"SYST:LOCA".to_vec();
        unlock.extend(&config.write_termination);
        Self {
            transport,
            stop,
            unlock_on_close: Some(unlock),
            close_timeout: config
                .write_timeout
                .min(Duration::from_millis(100)),
        }
    }
    fn check_cancellation(&self) -> Result<()> {
        if self.stop.load(Ordering::Relaxed) {
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into())
        } else {
            Ok(())
        }
    }
}
impl<T: Transport> Transport for Cancellable<T> {
    fn write_all(&mut self, b: &[u8]) -> Result<()> {
        self.check_cancellation()?;
        self.transport.write_all(b)
    }
    fn read(&mut self, b: &mut [u8]) -> Result<usize> {
        self.check_cancellation()?;
        self.transport.read(b)
    }
    fn set_timeout(&mut self, t: Duration) -> Result<()> {
        self.check_cancellation()?;
        self.transport.set_timeout(t)
    }
    fn close(&mut self) -> Result<()> {
        // Final write bypasses cancellation and SCPI delays. Its unread ACK may
        // survive closing; the next connection's initial IDN handshake handles it.
        let unlock = match self.unlock_on_close.take() {
            Some(command) => self
                .transport
                .set_timeout(self.close_timeout)
                .and_then(|()| self.transport.write_all(&command)),
            None => Ok(()),
        };
        let close = self.transport.close();
        unlock.and(close)
    }
}
