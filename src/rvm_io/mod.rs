mod bits;
mod rcore;
use alloc::sync::Arc;
use rcore::*;
use rcore_user::io::*;
use rcore_user::syscall::*;
use rvm::MmioPacket;
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

const RVM_RISCV_SET_SSIP: u32 = 0;
const RVM_RISCV_CLEAR_SSIP: u32 = 1;
const RVM_RISCV_SET_SEIP: u32 = 2;
const RVM_RISCV_CLEAR_SEIP: u32 = 3;

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
    SendInterruptError(i32),
    HandleMMIOError(i32),
    ReadStateError(i32),
    WriteStateError(i32),
}
use RVMError::*;
pub type Result<T> = core::result::Result<T, RVMError>;
impl RVM {
    pub fn new(path: &str) -> Result<RVM> {
        let fd = sys_open(path, O_RDWR);
        if fd < 0 {
            return Err(OpenRVMDeviceError(fd));
        }
        let fd = fd as usize;
        let vmid = sys_ioctl(fd as usize, RVM_GUEST_CREATE, 0);
        if vmid < 0 {
            return Err(CreateGuestError(vmid));
        }
        let vmid = vmid as usize;
        return Ok(RVM { fd, vmid });
    }
    pub fn add_memory_region(&self, gpa: u64, len: usize) -> Result<MemoryRegion> {
        let args = RvmGuestAddMemoryRegionArgs {
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
    fn interrupt(&self, vcpu_id: u16, arg: u32) -> Result<()> {
        let args = RvmVcpuInterruptArgs {
            vcpu_id,
            vector: arg,
        };
        let ret = sys_ioctl(self.fd, RVM_VCPU_INTERRUPT, &args as *const _ as usize);
        if ret < 0 {
            return Err(SendInterruptError(ret));
        }
        return Ok(());
    }
    pub fn set_interrupt_state(&self, vcpu_id: u16, sip: bool, eip: bool) -> Result<()> {
        if sip {
            self.interrupt(vcpu_id, RVM_RISCV_SET_SSIP)?;
        } else {
            self.interrupt(vcpu_id, RVM_RISCV_CLEAR_SSIP)?;
        }
        if eip {
            self.interrupt(vcpu_id, RVM_RISCV_SET_SEIP)?;
        } else {
            self.interrupt(vcpu_id, RVM_RISCV_CLEAR_SEIP)?;
        }
        Ok(())
    }
    pub fn modify_state<T>(
        &self,
        vcpu_id: u16,
        f: impl FnOnce(&mut rvm::VcpuState) -> Result<T>,
    ) -> Result<T> {
        let mut vcpu_state: core::mem::MaybeUninit<rvm::VcpuState> =
            core::mem::MaybeUninit::zeroed();
        let args = RvmVcpuStateArgs {
            vcpu_id: vcpu_id as u16,
            kind: rvm::VcpuReadWriteKind::VcpuState as u32,
            user_buf_ptr: vcpu_state.as_mut_ptr() as usize as u64,
            buf_size: core::mem::size_of::<rvm::VcpuState>() as u64,
        };
        let ret = sys_ioctl(self.fd, RVM_VCPU_READ_STATE, &args as *const _ as usize);
        if ret != 0 {
            return Err(ReadStateError(ret));
        }
        let mut vcpu_state = unsafe { vcpu_state.assume_init() };
        let val = f(&mut vcpu_state)?;
        let args = RvmVcpuStateArgs {
            vcpu_id: vcpu_id as u16,
            kind: rvm::VcpuReadWriteKind::VcpuState as u32,
            user_buf_ptr: &vcpu_state as *const _ as usize as u64,
            buf_size: core::mem::size_of::<rvm::VcpuState>() as u64,
        };
        let ret = sys_ioctl(self.fd, RVM_VCPU_WRITE_STATE, &args as *const _ as usize);
        if ret != 0 {
            return Err(WriteStateError(ret));
        }
        Ok(val)
    }
    fn incr_pc(&self, vcpu_id: u16, pc_incr: usize) -> Result<()> {
        self.modify_state(vcpu_id, |state| {
            //println!("increasing pc {} by {}", state.ctx.sepc, pc_incr);
            state.ctx.sepc += pc_incr;
            Ok(())
        })
    }
    fn assign_value_to_register_and_incr_pc<T: bits::BitExtendToUsize>(
        &self,
        vcpu_id: u16,
        reg_id: u8,
        val: T,
        sign_extension: bool,
        pc_incr: usize,
    ) -> Result<()> {
        self.modify_state(vcpu_id, |state| {
            let converted_val = val.to_usize(sign_extension);
            //println!("writing val {} to reg {} and increasing pc {} by {}", converted_val, reg_id, state.ctx.sepc, pc_incr);
            state.ctx.set(reg_id, converted_val);
            state.ctx.sepc += pc_incr;
            Ok(())
        })
    }
    // Handle mmio fault in parsed form.
    // Automatically does pc-increment, register writeback and sign/zero extension for you.
    pub fn handle_mmio_fault_with(
        &self,
        vcpu_id: u16,
        packet: &MmioPacket,
        handler: impl FnOnce(&mut devices::MMIOAccess) -> Option<bool>,
    ) -> Result<()> {
        let insn_len = packet.inst_len as usize;
        match (packet.access_size, packet.read) {
            // writes
            (1, false) => {
                if let Some(true) = handler(&mut devices::MMIOAccess::StoreByte(packet.data as u8))
                {
                    return self.incr_pc(vcpu_id, insn_len);
                } else {
                    return Err(HandleMMIOError(5));
                }
            }
            (2, false) => {
                if let Some(true) = handler(&mut devices::MMIOAccess::StoreHalf(packet.data as u16))
                {
                    return self.incr_pc(vcpu_id, insn_len);
                } else {
                    return Err(HandleMMIOError(5));
                }
            }
            (4, false) => {
                if let Some(true) = handler(&mut devices::MMIOAccess::StoreWord(packet.data as u32))
                {
                    return self.incr_pc(vcpu_id, insn_len);
                } else {
                    return Err(HandleMMIOError(5));
                }
            }
            (8, false) => {
                if let Some(true) =
                    handler(&mut devices::MMIOAccess::StoreDword(packet.data as u64))
                {
                    return self.incr_pc(vcpu_id, insn_len);
                } else {
                    return Err(HandleMMIOError(5));
                }
            }
            // reads
            (1, true) => {
                let mut val = 0;
                if let Some(true) = handler(&mut devices::MMIOAccess::LoadByte(&mut val)) {
                    return self.assign_value_to_register_and_incr_pc(
                        vcpu_id,
                        packet.dstreg,
                        val,
                        packet.extension,
                        insn_len,
                    );
                } else {
                    return Err(HandleMMIOError(1));
                }
            }
            (2, true) => {
                let mut val = 0;
                if let Some(true) = handler(&mut devices::MMIOAccess::LoadHalf(&mut val)) {
                    return self.assign_value_to_register_and_incr_pc(
                        vcpu_id,
                        packet.dstreg,
                        val,
                        packet.extension,
                        insn_len,
                    );
                } else {
                    return Err(HandleMMIOError(2));
                }
            }
            (4, true) => {
                let mut val = 0;
                if let Some(true) = handler(&mut devices::MMIOAccess::LoadWord(&mut val)) {
                    return self.assign_value_to_register_and_incr_pc(
                        vcpu_id,
                        packet.dstreg,
                        val,
                        packet.extension,
                        insn_len,
                    );
                } else {
                    return Err(HandleMMIOError(3));
                }
            }
            (8, true) => {
                let mut val = 0;
                if let Some(true) = handler(&mut devices::MMIOAccess::LoadDword(&mut val)) {
                    return self.assign_value_to_register_and_incr_pc(
                        vcpu_id,
                        packet.dstreg,
                        val,
                        packet.extension,
                        insn_len,
                    );
                } else {
                    return Err(HandleMMIOError(4));
                }
            }
            _ => {}
        }
        Err(HandleMMIOError(-1))
    }
}
