#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![cfg_attr(feature = "const_fn", feature(const_fn, const_unsafe_cell_new, const_atomic_usize_new))]

#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

pub use mutex::*;
pub use rw_lock::*;

#[cfg(feature = "once")]
pub use once::*;

mod mutex;
mod rw_lock;

#[cfg(feature = "once")]
mod once;
