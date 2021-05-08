use super::super::*;
use super::*;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicU32 as AtomicReg;
use core::sync::atomic::Ordering::*;
use spin::Mutex;
const MAXIMAL_INTERRUPT_GROUP: usize = 1;
#[derive(Default, Debug)]
pub struct PLICInterruptX32 {
    pub pending: AtomicReg,
    pub priority: [AtomicReg; 32],
}
#[derive(Default, Debug)]
pub struct PLICContext {
    pub enable_bits: [AtomicReg; MAXIMAL_INTERRUPT_GROUP],
    pub claimed_bits: [AtomicReg; MAXIMAL_INTERRUPT_GROUP],
    pub threshold: AtomicReg,
    pub eip: AtomicBool,
}

fn atomic_set_bit(reg: &AtomicReg, n: u8, bit: bool) {
    reg.fetch_update(
        |x| {
            let mask = 1u32 << (n as u32);
            if bit {
                Some(x | mask)
            } else {
                Some(x & (!mask))
            }
        },
        Relaxed,
        Relaxed,
    )
    .unwrap();
}
fn get_bit(reg: &AtomicReg, n: u8) -> bool {
    (reg.load(Relaxed) >> n) > 0
}

// PLIC interrupt handler for RISC-V.
// Maximal 31 interrupts and 2 contexts supported.
pub struct PLIC {
    sources: BTreeMap<usize, Arc<dyn Device>>,
    interrupts: [PLICInterruptX32; MAXIMAL_INTERRUPT_GROUP],
    contexts: [PLICContext; 2],
    // when claiming: lock.
    claim_searching: Mutex<()>,
}
impl PLIC {
    pub fn new(devices: BTreeMap<usize, Arc<dyn Device>>) -> Self {
        PLIC {
            sources: devices,
            interrupts: Default::default(),
            contexts: Default::default(),
            claim_searching: Mutex::new(()),
        }
    }
    pub fn eip(&self, ctx: usize) -> bool {
        self.get_context(ctx)
            .expect("bad context")
            .eip
            .load(Relaxed)
    }
    pub fn update_eip_for_context(&self, ctx: usize) {
        for (irq_id, d) in self.sources.iter() {
            let (interrupt_slice, iid) = self
                .get_interrupt_slice(*irq_id)
                .expect("bad device irq assignment");
            atomic_set_bit(&interrupt_slice.pending, iid as u8, d.has_interrupt())
        }
        //let claim_lock = self.claim_searching.read();
        let context_slice = self.get_context(ctx).expect("bad context");
        for igroup in 0..MAXIMAL_INTERRUPT_GROUP {
            let enabled = context_slice.enable_bits[igroup].load(Relaxed);
            let pending = self.interrupts[igroup].pending.load(Relaxed);
            let claimed = context_slice.claimed_bits[igroup].load(Relaxed);
            let enabled_pending_but_unclaimed = enabled & pending & !claimed;
            if enabled_pending_but_unclaimed != 0 {
                for i in 0..31 {
                    if ((enabled_pending_but_unclaimed >> i) & 1) == 1 {
                        let priority = self.interrupts[igroup].priority[i].load(Relaxed);
                        let threshold = context_slice.threshold.load(Relaxed);
                        if priority > 0 && priority > threshold {
                            context_slice.eip.store(true, Relaxed);
                            return;
                        }
                    }
                }
            }
        }
        context_slice.eip.store(false, Relaxed);
    }
    pub fn claim_interrupt(&self, ctx: usize) -> usize {
        let claim_lock = self.claim_searching.lock();
        let context_slice = self.get_context(ctx).expect("bad context");
        let mut highest_priority = 0;
        let mut chosen_irq = 0;
        for igroup in 0..MAXIMAL_INTERRUPT_GROUP {
            let enabled = context_slice.enable_bits[igroup].load(Relaxed);
            let pending = self.interrupts[igroup].pending.load(Relaxed);
            let claimed = context_slice.claimed_bits[igroup].load(Relaxed);
            let enabled_pending_but_unclaimed = enabled & pending & !claimed;
            if enabled_pending_but_unclaimed != 0 {
                for i in 0..31 {
                    if ((enabled_pending_but_unclaimed >> i) & 1) == 1 {
                        let priority = self.interrupts[igroup].priority[i].load(Relaxed);
                        if priority > highest_priority {
                            highest_priority = priority;
                            chosen_irq = igroup * 32 + i;
                        }
                    }
                }
            }
        }
        if chosen_irq == 0 {
            return 0;
        }
        let (chosen_slice, chosen_iid) = self.get_interrupt_slice(chosen_irq).unwrap();
        atomic_set_bit(&chosen_slice.pending, chosen_iid as u8, false);
        atomic_set_bit(
            &context_slice.claimed_bits[chosen_irq / 32],
            chosen_iid as u8,
            true,
        );
        drop(claim_lock);
        //self.update_eip_for_context(ctx);
        return chosen_irq;
    }
    pub fn complete_interrupt(&self, ctx: usize, irq: usize) {
        let context_slice = self.get_context(ctx).expect("bad context");
        atomic_set_bit(
            &context_slice.claimed_bits[irq / 32],
            (irq % 32) as u8,
            false,
        );
        //self.update_eip_for_context(ctx);
    }
    #[inline]
    pub fn get_interrupt_slice(&self, id: usize) -> Option<(&PLICInterruptX32, usize)> {
        let slice_id = id / 32;
        let in_slice_id = id % 32;
        Some((self.interrupts.get(slice_id)?, in_slice_id))
    }
    #[inline]
    pub fn get_context(&self, id: usize) -> Option<&PLICContext> {
        self.contexts.get(id)
    }
}

#[derive(Copy, Clone, Debug)]
enum PLICMemoryMap {
    Priority { source: usize },
    PendingX32 { source_div32: usize },
    EnableX32 { ctx: usize, source_div32: usize },
    Threshold { ctx: usize },
    ClaimComplete { ctx: usize },
}

impl PLICMemoryMap {
    // The mappping is complete.
    pub fn parse(offset: usize) -> Option<PLICMemoryMap> {
        use PLICMemoryMap::*;
        if offset < 0x1000 {
            return Some(Priority { source: offset / 4 });
        }
        if offset >= 0x1000 && offset < 0x1080 {
            return Some(PendingX32 {
                source_div32: (offset - 0x1000) / 4,
            });
        }
        if offset >= 0x2000 && offset < 0x1f2000 {
            let o = offset - 0x2000;
            let ctx = o / 0x80;
            let source_div32 = (o % 0x80) / 4;
            return Some(EnableX32 { ctx, source_div32 });
        }
        if offset >= 0x200000 && offset < 0x4000000 {
            let o = offset - 0x200000;
            let ctx = o / 0x1000;
            let register = o % 0x1000;
            if register < 4 {
                return Some(Threshold { ctx });
            }
            if register < 8 {
                return Some(ClaimComplete { ctx });
            }
        }
        return None;
    }
}

const VS_CONTEXT: usize = 1;
impl Device for PLIC {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn handle_mmio(&self, offset: usize, access: &mut MMIOAccess) -> Option<bool> {
        use PLICMemoryMap::*;
        if let Some(x) = PLICMemoryMap::parse(offset) {
            match access {
                MMIOAccess::LoadWord(ret) => match x {
                    Priority { source } => {
                        let (interrupt_slice, iid) = self.get_interrupt_slice(source)?;
                        **ret = interrupt_slice.priority[iid].load(Relaxed);
                        return Some(true);
                    }
                    EnableX32 { source_div32, ctx } => {
                        **ret = self
                            .get_context(ctx)?
                            .enable_bits
                            .get(source_div32)?
                            .load(Relaxed);
                        return Some(true);
                    }
                    Threshold { ctx } => {
                        **ret = self.get_context(ctx)?.threshold.load(Relaxed);
                        return Some(true);
                    }
                    ClaimComplete { ctx } => {
                        **ret = self.claim_interrupt(ctx) as u32;
                        return Some(true);
                    }
                    PendingX32 { source_div32 } => {
                        let (interrupt_slice, _iid) =
                            self.get_interrupt_slice(source_div32 * 32)?;
                        **ret = interrupt_slice.pending.load(Relaxed);
                        return Some(true);
                    }
                },
                MMIOAccess::StoreWord(val) => {
                    match x {
                        Priority { source } => {
                            let (interrupt_slice, iid) = self.get_interrupt_slice(source)?;
                            interrupt_slice.priority[iid].store(*val, Relaxed);
                            //for ctx in 0..self.contexts.len(){
                            //self.update_eip_for_context(ctx);
                            //}
                            return Some(true);
                        }
                        EnableX32 { source_div32, ctx } => {
                            let ctxs = self.get_context(ctx)?;
                            ctxs.enable_bits.get(source_div32)?.store(*val, Relaxed);
                            //self.update_eip_for_context(ctx);
                            return Some(true);
                        }
                        Threshold { ctx } => {
                            self.get_context(ctx)?.threshold.store(*val, Relaxed);
                            //self.update_eip_for_context(ctx);
                            return Some(true);
                        }
                        ClaimComplete { ctx } => {
                            self.complete_interrupt(ctx, (*val) as usize);
                            return Some(true);
                        }
                        _ => {
                            return None;
                        }
                    }
                }
                _ => {
                    // malformed access.
                    return None;
                }
            }
        }

        Some(false)
    }
    fn mmio_region_size(&self) -> usize {
        0x4000000
    }
    fn has_interrupt(&self) -> bool {
        self.update_eip_for_context(VS_CONTEXT);
        self.eip(VS_CONTEXT)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    trait MockMemOps {
        fn load(&self, addr: usize) -> Option<u32>;
        fn store(&self, addr: usize, val: u32) -> Option<()>;
    }
    impl MockMemOps for PLIC {
        fn load(&self, addr: usize) -> Option<u32> {
            if addr % 4 != 0 {
                return None;
            }
            let mut ret = 0;
            if !self.handle_mmio(addr, &mut MMIOAccess::LoadWord(&mut ret))? {
                return None;
            }
            Some(ret)
        }
        fn store(&self, addr: usize, val: u32) -> Option<()> {
            if addr % 4 != 0 {
                return None;
            }
            if !self.handle_mmio(addr, &mut MMIOAccess::StoreWord(val))? {
                return None;
            }
            Some(())
        }
    }
    struct MockDevice(Arc<AtomicBool>);
    impl Device for MockDevice {
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
        fn has_interrupt(&self) -> bool {
            self.0.load(Relaxed)
        }
    }
    #[test]
    fn basic_test() {
        let flag = Arc::new(AtomicBool::new(false));
        let mock: Arc<dyn Device> = Arc::new(MockDevice(Arc::clone(&flag)));
        const INTERRUPT_ID: usize = 10;
        let mut tree = BTreeMap::new();
        tree.insert(INTERRUPT_ID, mock);
        let plic = PLIC::new(tree);
        assert_eq!(
            plic.has_interrupt(),
            false,
            "Initially there is no interrupt."
        );

        plic.store(0x2080, plic.load(0x2080).unwrap() | 1 << INTERRUPT_ID)
            .unwrap(); // enable irq for context 1.
        plic.store(INTERRUPT_ID * 4, 7).unwrap();
        assert_eq!(plic.has_interrupt(), false, "Still no interrupt.");
        flag.store(true, Relaxed);
        assert_eq!(plic.has_interrupt(), true, "Interrupt captured.");
        let pending = plic.load(0x1000).unwrap();
        assert_eq!(pending, 1 << INTERRUPT_ID, "The irq is pending.");
        let claim = plic.load(0x201004).unwrap();
        assert_eq!(claim, INTERRUPT_ID as u32, "Claimed.");
        assert_eq!(
            plic.has_interrupt(),
            false,
            "Interrupt captured but claimed."
        );
        plic.store(0x201004, claim);
        assert_eq!(
            plic.has_interrupt(),
            true,
            "After complete, the interrupt is there again."
        );
        let claim = plic.load(0x201004).unwrap();
        assert_eq!(claim, INTERRUPT_ID as u32, "Claimed #2.");
        assert_eq!(
            plic.has_interrupt(),
            false,
            "Interrupt captured but claimed #2."
        );
        let another_claim = plic.load(0x201004).unwrap();
        assert_eq!(another_claim, 0, "Can't claim one interrupt again.");
        flag.store(false, Relaxed);
        plic.store(0x201004, claim);
        assert_eq!(plic.has_interrupt(), false, "Really no interrupt.");
        let claim = plic.load(0x201004).unwrap();
        assert_eq!(claim, 0, "No interrupt.");
    }
}
