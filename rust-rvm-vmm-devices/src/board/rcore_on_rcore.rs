use crate::irq::plic::PLIC;
use crate::serial::uart16650::Uart16650;
use crate::serial::{BlockingConsole, Console, SingleCharBufferedConsole};
use crate::Device;
use crate::MMIOBank;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
/// device tree and mmio bank.
const SERIAL_IRQ: usize = 10;
const SERIAL_MMIO: usize = 0x1000000;
const PLIC_MMIO: usize = 0xc000000;

pub fn rcore_on_rcore(
    blocking_console: Arc<dyn Console>,
) -> (MMIOBank, Arc<dyn Device>, Arc<dyn Device>, &'static [u8]) {
    let serial: Arc<dyn Device> = Arc::new(Uart16650::new(Arc::clone(&blocking_console)));
    let mut irqtree = BTreeMap::new();
    irqtree.insert(SERIAL_IRQ, Arc::clone(&serial));
    let irc: Arc<dyn Device> = Arc::new(PLIC::new(irqtree));
    let mut bank = MMIOBank::new();
    bank.add_device(PLIC_MMIO, Arc::clone(&irc));
    bank.add_device(SERIAL_MMIO, Arc::clone(&serial));
    (bank, irc, serial, include_bytes!("rcore_on_rcore.dtb"))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::MMIOAccess;
    trait MockMemOps {
        fn lw(&self, addr: usize) -> Option<u32>;
        fn sw(&self, addr: usize, val: u32) -> Option<()>;
        fn lb(&self, addr: usize) -> Option<u8>;
        fn sb(&self, addr: usize, val: u8) -> Option<()>;
    }
    impl MockMemOps for MMIOBank {
        fn lw(&self, addr: usize) -> Option<u32> {
            if addr % 4 != 0 {
                return None;
            }
            let mut ret = 0;
            if !self.handle_mmio(addr, &mut MMIOAccess::LoadWord(&mut ret))? {
                return None;
            }
            Some(ret)
        }
        fn sw(&self, addr: usize, val: u32) -> Option<()> {
            if addr % 4 != 0 {
                return None;
            }
            if !self.handle_mmio(addr, &mut MMIOAccess::StoreWord(val))? {
                return None;
            }
            Some(())
        }
        fn lb(&self, addr: usize) -> Option<u8> {
            let mut ret = 0;
            if !self.handle_mmio(addr, &mut MMIOAccess::LoadByte(&mut ret))? {
                return None;
            }
            Some(ret)
        }
        fn sb(&self, addr: usize, val: u8) -> Option<()> {
            if !self.handle_mmio(addr, &mut MMIOAccess::StoreByte(val))? {
                return None;
            }
            Some(())
        }
    }
    use spin::Mutex;
    use std::sync::mpsc::{channel, Receiver, Sender};
    struct StdChannelConsole {
        rx: Mutex<Receiver<u8>>,
        tx: Mutex<Sender<u8>>,
        output_buffer: Mutex<Vec<u8>>,
    }
    impl StdChannelConsole {
        pub fn new() -> Self {
            let (tx, rx) = channel();
            StdChannelConsole {
                rx: Mutex::new(rx),
                tx: Mutex::new(tx),
                output_buffer: Mutex::new(Vec::new()),
            }
        }
        pub fn output(&self) -> Vec<u8> {
            self.output_buffer.lock().clone()
        }
        pub fn send(&self, chr: u8) {
            self.tx.lock().send(chr);
        }
    }
    impl BlockingConsole for StdChannelConsole {
        fn getc(&self) -> u8 {
            self.rx.lock().recv().unwrap_or(0)
        }
        fn putc(&self, chr: u8) {
            self.output_buffer.lock().push(chr);
        }
        fn start_task<F: FnOnce() -> ()>(f: F)
        where
            F: Send + 'static,
        {
            std::thread::spawn(f);
        }
    }
    #[test]
    fn test_system() {
        use crate::serial::uart16650::*;
        let stdconsole = Arc::new(StdChannelConsole::new());
        let console: Arc<dyn Console> =
            Arc::new(SingleCharBufferedConsole::new(Arc::clone(&stdconsole)));
        console
            .as_any()
            .downcast_ref::<SingleCharBufferedConsole<StdChannelConsole>>()
            .unwrap()
            .start(Arc::clone(&console));
        let (board, plic_i, serial, _) = rcore_on_rcore(Arc::clone(&console));
        // storing unrelated registers. taken from rcore.
        board.sb(SERIAL_MMIO + COM_FCR * MULTIPLIER, 0).unwrap();
        board
            .sb(SERIAL_MMIO + COM_LCR * MULTIPLIER, COM_LCR_DLAB)
            .unwrap();
        board
            .sb(
                SERIAL_MMIO + COM_LCR * MULTIPLIER,
                COM_LCR_WLEN8 & !COM_LCR_DLAB,
            )
            .unwrap();
        board.sb(SERIAL_MMIO + COM_MCR * MULTIPLIER, 0).unwrap();
        // enable interrupt
        board
            .sb(SERIAL_MMIO + COM_IER * MULTIPLIER, COM_IER_RDI)
            .unwrap();
        // register interrupt
        board
            .sw(
                PLIC_MMIO + 0x2080,
                board.lw(PLIC_MMIO + 0x2080).unwrap() | 1 << SERIAL_IRQ,
            )
            .unwrap(); // enable irq for context 1.
        board.sw(PLIC_MMIO + SERIAL_IRQ * 4, 7).unwrap(); // set priority

        // device ready.
        assert_eq!(
            plic_i.has_interrupt(),
            false,
            "Till now still no interrupt."
        );
        assert_ne!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_TXRDY,
            0,
            "Tx always ready."
        );
        assert_eq!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "No data."
        );
        let DATA = 1;
        stdconsole.send(DATA);
        // wait 100 ms.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(plic_i.has_interrupt(), true, "Interrupt captured.");
        let pending = board.lw(PLIC_MMIO + 0x1000).unwrap();
        assert_eq!(pending, 1 << SERIAL_IRQ, "The irq is pending.");
        let claim = board.lw(PLIC_MMIO + 0x201004).unwrap();
        assert_eq!(claim, SERIAL_IRQ as u32, "Claimed.");
        assert_eq!(
            plic_i.has_interrupt(),
            false,
            "Interrupt captured but claimed."
        );
        assert_ne!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "Data."
        );
        let data = board.lb(SERIAL_MMIO + COM_RX * MULTIPLIER).unwrap();
        assert_eq!(data, DATA, "Data got.");
        assert_eq!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "No data."
        );

        board.sw(PLIC_MMIO + 0x201004, claim).unwrap();
        assert_eq!(plic_i.has_interrupt(), false, "Interrupt handled.");
        // test: two chars, two handlings.
        stdconsole.send(DATA);
        stdconsole.send(DATA + 1);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(plic_i.has_interrupt(), true, "Interrupt captured.");
        let pending = board.lw(PLIC_MMIO + 0x1000).unwrap();
        assert_eq!(pending, 1 << SERIAL_IRQ, "The irq is pending.");
        let claim = board.lw(PLIC_MMIO + 0x201004).unwrap();
        assert_eq!(claim, SERIAL_IRQ as u32, "Claimed.");
        assert_eq!(
            plic_i.has_interrupt(),
            false,
            "Interrupt captured but claimed."
        );
        assert_ne!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "Data #1."
        );
        let data = board.lb(SERIAL_MMIO + COM_RX * MULTIPLIER).unwrap();
        assert_eq!(data, DATA, "Data #1 got.");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_ne!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "Still data."
        );
        board.sw(PLIC_MMIO + 0x201004, claim).unwrap();
        assert_eq!(
            plic_i.has_interrupt(),
            true,
            "We need to handle interrupt again."
        );

        let pending = board.lw(PLIC_MMIO + 0x1000).unwrap();
        assert_eq!(pending, 1 << SERIAL_IRQ, "The irq is pending.");
        let claim = board.lw(PLIC_MMIO + 0x201004).unwrap();
        assert_eq!(claim, SERIAL_IRQ as u32, "Claimed.");
        assert_eq!(
            plic_i.has_interrupt(),
            false,
            "Interrupt captured but claimed."
        );
        assert_ne!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "Data #2."
        );
        let data = board.lb(SERIAL_MMIO + COM_RX * MULTIPLIER).unwrap();
        assert_eq!(data, DATA + 1, "Data #2 got.");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(
            board.lb(SERIAL_MMIO + COM_LSR * MULTIPLIER).unwrap() & COM_LSR_DATA,
            0,
            "No data."
        );
        board.sw(PLIC_MMIO + 0x201004, claim).unwrap();
        assert_eq!(
            plic_i.has_interrupt(),
            false,
            "Handled two interrupts. Exit."
        );
    }
}
