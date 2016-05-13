#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![cfg_attr(feature = "asm", feature(asm))]
#![cfg_attr(feature = "core_intrinsics", feature(core_intrinsics))]
#![feature(const_fn)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

pub use mutex::*;
pub use rw_lock::*;
pub use once::*;

mod mutex;
mod rw_lock;
mod once;

mod util;
