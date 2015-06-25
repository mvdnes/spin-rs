#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![cfg_attr(feature = "no_std", feature(no_std, const_fn, core))]
#![cfg_attr(feature = "no_std", no_std)]

#[cfg(all(feature = "no_std", test))]
extern crate std;

#[cfg(feature = "no_std")]
#[macro_use]
extern crate core;

pub use mutex::*;
pub use rw_lock::*;

mod mutex;
mod rw_lock;
