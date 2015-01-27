#![crate_type = "lib"]
#![feature(unsafe_destructor)]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![no_std]

#[cfg(test)]
extern crate std;

#[macro_use] #[allow(unstable)]
extern crate core;

pub use mutex::*;
pub use rw_lock::*;

mod mutex;
mod rw_lock;

#[cfg(not(test))]
mod std {
    pub use core::*;
}
