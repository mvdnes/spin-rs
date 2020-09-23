#![crate_type = "lib"]
#![warn(missing_docs)]

//! Synchronization primitives based on spinning

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

/// Spin synchronisation primitives, but compatible with `lock_api`.
#[cfg(feature = "lock_api1")]
pub mod lock_api {
    /// `lock_api`-compatible version of [`crate::Mutex`].
    pub type Mutex<T> = lock_api::Mutex<crate::Mutex<()>, T>;

    /// `lock_api`-compatible version of [`crate::RwLock`].
    pub type RwLock<T> = lock_api::RwLock<crate::RwLock<()>, T>;
}
