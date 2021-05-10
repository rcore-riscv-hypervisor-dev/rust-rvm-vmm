use crate::devices::serial::BlockingConsole;
use alloc::sync::Arc;
use rcore_user::io::*;
use rcore_user::syscall::*;
use rcore_user::thread::spawn;
pub fn putc_uart2(fd: usize, c: u8) {
    sys_write(fd, &c, 1);
}

pub fn getc_uart2(fd: usize) -> u8 {
    let mut c = 0u8;
    loop {
        sys_sleep(100);
        let len = sys_read(fd, &mut c, 1);
        match len {
            1 => {
                //println!("character {} fetched", c);
                return c;
            }
            0 => continue,
            _ => panic!("read uart2 len = {}", len),
        }
    }
}
pub struct RcoreConsole(usize);

impl RcoreConsole {
    pub fn new() -> Self {
        let fd = sys_open("/dev/ttyS1", O_RDWR);
        if fd < 0 {
            panic!("can't open second serial.");
        }
        RcoreConsole(fd as usize)
    }
    pub fn try_getc(&self) -> Option<u8> {
        let mut c = 0u8;
        let len = sys_read(self.0, &mut c, 1);
        match len {
            1 => {
                //println!("character {} fetched", c);
                return Some(c);
            }
            0 => None,
            _ => panic!("read uart2 len = {}", len),
        }
    }
}

impl BlockingConsole for RcoreConsole {
    fn getc(&self) -> u8 {
        getc_uart2(self.0)
    }
    fn putc(&self, chr: u8) {
        putc_uart2(self.0, chr)
    }
    fn start_task<F: FnOnce() -> ()>(f: F)
    where
        F: Send + 'static,
    {
        spawn(f);
    }
}
pub fn start_rcore_serial() -> Arc<dyn devices::serial::Console> {
    use devices::serial::*;
    let stdconsole = Arc::new(RcoreConsole::new());
    let console: Arc<dyn Console> =
        Arc::new(SingleCharBufferedConsole::new(Arc::clone(&stdconsole)));

    console
}
