#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![feature(no_std, const_fn, core, core_prelude)]
#![no_std]

#[cfg(test)]
extern crate std;

#[macro_use]
extern crate core;

pub use mutex::*;
pub use rw_lock::*;

mod mutex;
mod rw_lock;
