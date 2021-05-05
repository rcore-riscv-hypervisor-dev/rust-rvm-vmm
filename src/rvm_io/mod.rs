mod rcore;
use alloc::sync::Arc;
use rcore::*;
use rcore_user::io::*;
use rcore_user::syscall::*;
use rvm::RvmError as RawRvmError;
use rvm::RvmExitPacket;
pub const RVM_IO: usize = 0xAE00;
pub const RVM_GUEST_CREATE: usize = RVM_IO + 0x01;
pub const RVM_GUEST_ADD_MEMORY_REGION: usize = RVM_IO + 0x02;
pub const RVM_GUEST_SET_TRAP: usize = RVM_IO + 0x03;
pub const RVM_VCPU_CREATE: usize = RVM_IO + 0x11;
pub const RVM_VCPU_RESUME: usize = RVM_IO + 0x12;
pub const RVM_VCPU_READ_STATE: usize = RVM_IO + 0x13;
pub const RVM_VCPU_WRITE_STATE: usize = RVM_IO + 0x14;
pub const RVM_VCPU_INTERRUPT: usize = RVM_IO + 0x15;

pub struct RVM {
    fd: usize,
    vmid: usize,
}

pub struct MemoryRegion<'a> {
    pub gpa: u64,
    pub data: &'a mut [u8],
}

#[derive(Debug)]
pub enum RVMError {
    OpenRVMDeviceError(i32),
    CreateGuestError(i32),
    AddMemoryRegionError(i32),
    CreateVcpuError(i32),
    ResumeError(i32),
}
use RVMError::*;
pub type Result<T> = core::result::Result<T, RVMError>;
impl RVM {
    pub fn new(path: &str) -> Result<RVM> {
        let fd = 1;
        let fd = sys_open(path, O_RDWR);
        if fd < 0 {
            return Err(OpenRVMDeviceError(fd));
        }
        let fd = fd as usize;
        let vmid = 1;
        let vmid = sys_ioctl(fd as usize, RVM_GUEST_CREATE, 0);
        if vmid < 0 {
            return Err(CreateGuestError(vmid));
        }
        let vmid = vmid as usize;
        return Ok(RVM { fd, vmid });
    }
    pub fn add_memory_region(&self, gpa: u64, len: usize) -> Result<MemoryRegion> {
        let mut args = RvmGuestAddMemoryRegionArgs {
            guest_start_paddr: gpa,
            memory_size: len as u64,
            vmid: self.vmid as u16,
            userspace_addr: 0,
        };
        let ret = sys_ioctl(
            self.fd,
            RVM_GUEST_ADD_MEMORY_REGION,
            &args as *const _ as usize,
        );
        if ret < 0 {
            return Err(AddMemoryRegionError(ret));
        } else {
            return Ok(MemoryRegion {
                gpa,
                data: unsafe {
                    core::slice::from_raw_parts_mut(args.userspace_addr as *mut _, len)
                },
            });
        }
    }
    pub fn create_vcpu(&self, entry: u64) -> Result<u16> {
        let args = RvmVcpuCreateArgs {
            vmid: self.vmid as u16,
            entry,
        };
        let ret = sys_ioctl(self.fd, RVM_VCPU_CREATE, &args as *const _ as usize);
        if ret < 0 {
            return Err(CreateVcpuError(ret));
        }
        return Ok(ret as u16);
    }
    pub fn resume(&self, vcpu_id: u16) -> Result<RvmExitPacket> {
        let mut args: RvmVcpuResumeArgs = unsafe { core::mem::uninitialized() };
        args.vcpu_id = vcpu_id;
        let ret = sys_ioctl(self.fd, RVM_VCPU_RESUME, &mut args as *mut _ as usize);
        if ret < 0 {
            return Err(ResumeError(ret));
        } else {
            return Ok(args.packet);
        }
    }
}
