// rCore RVM inode arguments.
#[repr(C)]
#[derive(Debug)]
pub struct RvmVcpuCreateArgs {
    pub vmid: u16,
    pub entry: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct RvmGuestAddMemoryRegionArgs {
    pub vmid: u16,
    pub guest_start_paddr: u64,
    pub memory_size: u64,
    pub userspace_addr: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct RvmGuestSetTrapArgs {
    pub vmid: u16,
    pub kind: u32,
    pub addr: u64,
    pub size: u64,
    pub key: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct RvmVcpuResumeArgs {
    pub vcpu_id: u16,
    pub packet: rvm::RvmExitPacket,
}

#[repr(C)]
#[derive(Debug)]
pub struct RvmVcpuStateArgs {
    pub vcpu_id: u16,
    pub kind: u32,
    pub user_buf_ptr: u64,
    pub buf_size: u64,
}

#[repr(C)]
#[derive(Debug)]
pub struct RvmVcpuInterruptArgs {
    pub vcpu_id: u16,
    pub vector: u32,
}
