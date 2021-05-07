use crate::Device;
pub trait IrqDevice: Device {
    fn has_interrupt(&self) -> bool;
}

pub mod plic;
