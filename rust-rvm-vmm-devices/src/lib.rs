#![cfg_attr(not(test), no_std)]
#![feature(no_more_cas)]
pub mod device;
pub mod irq;

pub use device::*;

extern crate alloc;
extern crate core;
