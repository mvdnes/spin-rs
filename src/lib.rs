#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![feature(const_fn, asm)]
#![no_std]

#[cfg(test)]
extern crate std;

pub use mutex::*;
pub use rw_lock::*;

mod mutex;
mod rw_lock;
mod util;
