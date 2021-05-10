#![no_std]
#![no_main]

#[macro_use]
extern crate rcore_user;
extern crate alloc;
extern crate core;
extern crate rvm;
use alloc::sync::Arc;
use core::fmt::Write;
mod console;
mod rvm_io;

extern crate rust_rvm_vmm_devices as devices;

fn read_file(path: &str, buf: &mut [u8]) -> Result<usize, i32> {
    use rcore_user::io::*;
    use rcore_user::syscall::*;
    let fd = sys_open(path, O_RDONLY);
    if fd < 0 {
        return Err(fd);
    }
    let len = sys_read(fd as usize, buf.as_mut_ptr(), buf.len());
    sys_close(fd as usize);
    if len < 0 {
        return Err(len);
    }
    return Ok(len as usize);
}
use devices::serial::Console;
pub struct HeaplessWrite<T: AsRef<dyn Console>>(T);
impl<T: AsRef<dyn Console>> core::fmt::Write for HeaplessWrite<T> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.as_bytes() {
            self.0.as_ref().write(*c);
        }
        Ok(())
    }
}
use devices::Device;
fn rvm_main() -> rvm_io::Result<()> {
    rcore_user::syscall::enlarge_heap();
    println!("rust-rvm-vmm starting");
    let vm = Arc::new(rvm_io::RVM::new("/dev/rvm")?);
    let vm_image_path = "/vmm/rcore";
    let console = console::start_rcore_serial();
    let (mmio, irc, fdt) = devices::board::rcore_on_rcore::rcore_on_rcore(Arc::clone(&console));

    let mut writer = HeaplessWrite(&console);
    write!(writer, "hello, vmm").unwrap();
    let mem = vm.add_memory_region(0x80200000, 384 * 1024 * 1024)?;
    read_file(vm_image_path, mem.data).unwrap();

    let fdt_mem = vm.add_memory_region(0xa0000000, (fdt.len() + 4095) / 4096 * 4096)?;
    unsafe { core::ptr::copy_nonoverlapping(fdt.as_ptr(), fdt_mem.data.as_mut_ptr(), fdt.len()) };
    let vcpu = vm.create_vcpu(0x80200000)?;
    vm.modify_state(vcpu, |state| {
        state.ctx.a0 = 0;
        state.ctx.a1 = fdt_mem.gpa as usize;
        Ok(())
    })?;
    // new thread for read
    use console::RcoreConsole;
    use devices::serial::SingleCharBufferedConsole;

    let sc = console
        .as_any()
        .downcast_ref::<SingleCharBufferedConsole<RcoreConsole>>()
        .unwrap();

    println!("starting");

    rcore_user::ulib::sleep(1);
    loop {
        if let None = sc.try_read(false) {
            let rc = sc.get_underlying();
            if let Some(x) = rc.try_getc() {
                sc.notify_char(x);
            }
        }
        vm.set_interrupt_state(vcpu, false, irc.has_interrupt())
            .unwrap();
        let packet = vm.resume(vcpu)?;
        match packet.kind {
            rvm::RvmExitPacketKind::GuestEcall => {
                let ecall = unsafe { &packet.inner.ecall };
                if ecall.eid == 1 {
                    console.write(ecall.arg0 as u8);
                } else {
                    write!(
                        writer,
                        "[vmm] Bad ecall eid={} fid={}. Ignore.\n",
                        ecall.eid, ecall.fid
                    )
                    .unwrap();
                }
            }
            rvm::RvmExitPacketKind::GuestMmio => {
                let mmio_packet = unsafe { &packet.inner.mmio };
                vm.handle_mmio_fault_with(vcpu, &mmio_packet, |access| {
                    //println!("mmio packet {:?}", mmio_packet);
                    mmio.handle_mmio(mmio_packet.addr as usize, access)
                })?;
            }
            rvm::RvmExitPacketKind::GuestYield => {
                // inject interrupt.
            }
            _ => {
                println!("Bad exit. Exit.");
                break;
            }
        }
    }
    Ok(())
}

#[no_mangle]
fn main() {
    match rvm_main() {
        Ok(()) => {}
        Err(x) => {
            println!("Error in RVM: {:?}", x);
        }
    }
}
