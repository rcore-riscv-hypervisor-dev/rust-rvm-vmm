#![no_std]
#![no_main]

#[macro_use]
extern crate rcore_user;
extern crate rvm;
extern crate alloc;
extern crate core;
mod rvm_io;

fn read_file(path: &str, buf: &mut [u8])->Result<usize, i32>{
    use rcore_user::syscall::*;
    use rcore_user::io::*;
    let fd = sys_open(path, O_RDONLY);
    if fd<0{
        return Err(fd);
    }
    let len = sys_read(fd as usize, buf.as_mut_ptr(), buf.len());
    sys_close(fd as usize);
    if len<0{
        return Err(len);
    }
    return Ok(len as usize);
}
fn rvm_main()->rvm_io::Result<()>{
    println!("rust-rvm-vmm starting");
    let vm = rvm_io::RVM::new("/dev/rvm")?;
    let vm_image_path = "/vmm/rvloader.img";
    let mem = vm.add_memory_region(0x80200000, 128*1024*1024)?;
    read_file(vm_image_path, mem.data).unwrap();
    let vcpu = vm.create_vcpu(0x80200000)?;
    loop{
        let packet = vm.resume(vcpu)?;
        match packet.kind{
            rvm::RvmExitPacketKind::GuestEcall=>{
                let ecall = unsafe {&packet.inner.ecall};
                if ecall.eid==1{
                    rcore_user::io::putc(ecall.arg0 as u8);
                }else{
                    println!("Bad ecall eid={} fid={}. Ignore.", ecall.eid, ecall.fid);
                }
            }
            _=>{
                println!("Bad exit. Exit.");
                break;
            }
        }
    }
    Ok(())
}

#[no_mangle]
fn main() {
    match rvm_main(){
        Ok(())=>{}
        Err(x)=>{
            println!("Error in RVM: {:?}", x);
        }
    }
}
