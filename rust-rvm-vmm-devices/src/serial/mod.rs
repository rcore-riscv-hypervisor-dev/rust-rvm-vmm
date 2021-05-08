use alloc::sync::{Arc, Weak};
use core::sync::atomic::{AtomicBool, Ordering::*};
use spin::Mutex;
// The intermediate interface between serial and user input.
pub trait Console: Send + Sync {
    // Called by serial emulator to try to obtain a character.
    fn try_read(&self, pop: bool) -> Option<u8>;
    fn write(&self, chr: u8);
    // Called by serial provider (usually in another thread) to push one character into the buffer.
    fn notify_char(&self, chr: u8);
    // downcast
    fn as_any(&self) -> &dyn core::any::Any;
}

// Abstraction for a ``consle''. Exactly enough to fit into rcore_user.
pub trait BlockingConsole: Sync + Send {
    fn getc(&self) -> u8;
    fn putc(&self, chr: u8);
    fn start_task<F: FnOnce() -> ()>(f: F)
    where
        F: Send + 'static;
}
pub struct SingleCharBufferedConsole<T: BlockingConsole + 'static> {
    buffer: Mutex<Option<u8>>,
    full: AtomicBool,
    closed: AtomicBool,
    underlying: Arc<T>,
}

impl<T: BlockingConsole + 'static> SingleCharBufferedConsole<T> {
    pub fn new(underlying: Arc<T>) -> Self {
        SingleCharBufferedConsole {
            buffer: Mutex::new(None),
            full: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            underlying,
        }
    }
    pub fn start(&self, real_self: Arc<dyn Console>) {
        let p = Arc::downgrade(&real_self);
        let underlying = Arc::clone(&self.underlying);
        drop(real_self);
        T::start_task(move || loop {
            let chr = underlying.getc();
            if let Some(serial) = Weak::upgrade(&p) {
                serial
                    .as_any()
                    .downcast_ref::<Self>()
                    .unwrap()
                    .notify_char(chr);
            }
        });
    }
}

impl<T: BlockingConsole + 'static> Console for SingleCharBufferedConsole<T> {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn try_read(&self, pop: bool) -> Option<u8> {
        let mut chr = self.buffer.lock();
        let ch = chr.as_ref().copied();
        if pop {
            *chr = None;
            self.full.store(false, SeqCst);
        }
        ch
    }
    fn write(&self, chr: u8) {
        self.underlying.putc(chr);
    }
    fn notify_char(&self, chr: u8) {
        while self.full.load(SeqCst) && !self.closed.load(SeqCst) {
            // spin
        }
        *self.buffer.lock() = Some(chr);
        self.full.store(true, SeqCst);
    }
}

impl<T: BlockingConsole + 'static> Drop for SingleCharBufferedConsole<T> {
    fn drop(&mut self) {
        self.closed.store(true, SeqCst);
    }
}

pub mod uart16650;
