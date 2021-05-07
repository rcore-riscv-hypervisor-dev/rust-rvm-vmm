use alloc::sync::Arc;
use alloc::vec::Vec;
pub enum MMIOAccess<'a> {
    StoreByte(u8),
    LoadByte(&'a mut u8),
    StoreHalf(u16),
    LoadHalf(&'a mut u16),
    StoreWord(u32),
    LoadWord(&'a mut u32),
    StoreDword(u64),
    LoadDword(&'a mut u64),
}
pub trait Device {
    /// Try handle mmio.
    /// Return values: Some(true) for success handling, Some(false) for unrelated access, None for malformed access.
    fn handle_mmio(&self, offset: usize, access: &mut MMIOAccess) -> Option<bool> {
        Some(false)
    }
    // The size eseimation for mmio region of the device.
    fn mmio_region_size(&self) -> usize {
        0
    }
}

struct MMIODescription {
    base: usize,
    device: Arc<dyn Device>,
}

pub struct MMIOBank {
    devices: Vec<MMIODescription>,
}

impl MMIOBank {
    pub fn new() -> Self {
        MMIOBank {
            devices: Vec::new(),
        }
    }
}

impl Device for MMIOBank {
    fn handle_mmio(&self, offset: usize, access: &mut MMIOAccess) -> Option<bool> {
        for dev in self.devices.iter() {
            if offset >= dev.base && offset < dev.base + dev.device.mmio_region_size() {
                let o = offset - dev.base;
                if dev.device.handle_mmio(o, access)? {
                    return Some(true);
                }
            }
        }
        Some(false)
    }
    fn mmio_region_size(&self) -> usize {
        0
    }
}
