#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

#![feature(core)]

#![cfg_attr(not(feature = "std"), feature(no_std))]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(not(feature = "std"), test))]
extern crate std;

#[cfg(not(feature = "std"))]
#[macro_use]
extern crate core;

pub use mutex::*;
pub use rw_lock::*;

mod mutex;
mod rw_lock;
