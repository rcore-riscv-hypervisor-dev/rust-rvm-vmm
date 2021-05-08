use super::Console;
use crate::device::MMIOAccess;
use crate::Device;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering::*};
pub const COM_RX: usize = 0; // In:  Receive buffer (DLAB=0)
pub const COM_TX: usize = 0; // Out: Transmit buffer (DLAB=0)
pub const COM_DLL: usize = 0; // Out: Divisor Latch Low (DLAB=1)
pub const COM_DLM: usize = 1; // Out: Divisor Latch High (DLAB=1)
pub const COM_IER: usize = 1; // Out: Interrupt Enable Register
pub const COM_IER_RDI: u8 = 0x01; // Enable receiver data interrupt
pub const COM_IIR: usize = 2; // In:  Interrupt ID Register
pub const COM_FCR: usize = 2; // Out: FIFO Control Register
pub const COM_LCR: usize = 3; // Out: Line Control Register
pub const COM_LCR_DLAB: u8 = 0x80; // Divisor latch access bit
pub const COM_LCR_WLEN8: u8 = 0x03; // Wordlength: 8 bits
pub const COM_MCR: usize = 4; // Out: Modem Control Register
pub const COM_MCR_RTS: u8 = 0x02; // RTS complement
pub const COM_MCR_DTR: u8 = 0x01; // DTR complement
pub const COM_MCR_OUT2: u8 = 0x08; // Out2 complement
pub const COM_LSR: usize = 5; // In:  Line Status Register
pub const COM_LSR_DATA: u8 = 0x01; // Data available
pub const COM_LSR_TXRDY: u8 = 0x20; // Transmit buffer avail
pub const COM_LSR_TSRE: u8 = 0x40; // Transmitter off
pub const MULTIPLIER: usize = 1 << 0;
/// Simple serial.
/// Just enough for rCore to run.
pub struct Uart16650 {
    console: Arc<dyn Console>,
    intr_enabled: AtomicBool,
}
impl Uart16650 {
    pub fn new(console: Arc<dyn Console>) -> Self {
        Uart16650 {
            console,
            intr_enabled: AtomicBool::new(false),
        }
    }
}
impl Device for Uart16650 {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn handle_mmio(&self, offset: usize, access: &mut MMIOAccess) -> Option<bool> {
        match access {
            MMIOAccess::LoadByte(ret) => match offset {
                _ if offset == COM_RX * MULTIPLIER => {
                    **ret = self.console.try_read(true).unwrap_or(0);
                }
                _ if offset == COM_LSR * MULTIPLIER => {
                    let mut x = COM_LSR_TXRDY;
                    if self.console.try_read(false).is_some() {
                        x |= COM_LSR_DATA;
                    }
                    **ret = x;
                }
                _ => {
                    **ret = 0;
                }
            },
            MMIOAccess::StoreByte(val) => match offset {
                _ if offset == COM_TX * MULTIPLIER => {
                    self.console.write(*val);
                }
                _ if offset == COM_IER * MULTIPLIER => {
                    self.intr_enabled.store(*val == COM_IER_RDI, Relaxed);
                }
                _ => {}
            },
            _ => {
                // malformed access.
                return None;
            }
        }
        // Ignored features.
        Some(true)
    }
    fn mmio_region_size(&self) -> usize {
        256
    }
    fn has_interrupt(&self) -> bool {
        if self.intr_enabled.load(Relaxed) {
            self.console.try_read(false).is_some()
        } else {
            false
        }
    }
}
