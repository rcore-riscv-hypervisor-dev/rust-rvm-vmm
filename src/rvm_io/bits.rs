pub trait BitExtendToUsize {
    fn to_usize(self, sign_ext: bool) -> usize;
}
macro_rules! impl_bit_extend {
    ($t: ty) => {
        impl BitExtendToUsize for $t {
            #[inline]
            fn to_usize(self, sign_ext: bool) -> usize {
                let mut val = self as usize;
                if sign_ext {
                    let msb: Self = (!((!0) >> 1)) & self;
                    if msb != 0 {
                        let max_entire_mask = !0usize;
                        let max_current_mask: Self = !0;
                        let new_mask = max_entire_mask ^ (max_current_mask as usize);
                        val = val | new_mask;
                    }
                }
                return val;
            }
        }
    };
}
impl_bit_extend!(u8);
impl_bit_extend!(u16);
impl_bit_extend!(u32);
impl_bit_extend!(u64);
