#![deny(missing_docs)]

//! This crate provides [spin-based](https://en.wikipedia.org/wiki/Spinlock) versions of the
//! primitives in `std::sync` and `std::lazy`. Because synchronization is done through spinning,
//! the primitives are suitable for use in `no_std` environments.
//!
//! # Features
//!
//! - `Mutex`, `RwLock`, `Once`/`SyncOnceCell`, and `SyncLazy` equivalents
//!
//! - Support for `no_std` environments
//!
//! - [`lock_api`](https://crates.io/crates/lock_api) compatibility
//!
//! - Upgradeable `RwLock` guards
//!
//! - Guards can be sent and shared between threads
//!
//! - Guard leaking
//!
//! # Relationship with `std::sync`
//!
//! While `spin` is not a drop-in replacement for `std::sync` (and
//! [should not be considered as such](https://matklad.github.io/2020/01/02/spinlocks-considered-harmful.html))
//! an effort is made to keep this crate reasonably consistent with `std::sync`.
//!
//! Many of the types defined in this crate have 'additional capabilities' when compared to `std::sync`:
//!
//! - Because spinning does not depend on the thread-driven model of `std::sync`, guards ([`MutexGuard`],
//!   [`RwLockReadGuard`], [`RwLockWriteGuard`], etc.) may be sent and shared between threads.
//!
//! - [`RwLockUpgradableGuard`] supports being upgrades into a [`RwLockWriteGuard`].
//!
//! - Guards support [leaking](https://doc.rust-lang.org/nomicon/leaking.html).
//!
//! - [`Once`] owns the value returned by its `call_once` initializer.
//!
//! - [`RwLock`] supports counting readers and writers.
//!
//! Conversely, the types in this crate do not have some of the features `std::sync` has:
//!
//! - Locks do not track [panic poisoning](https://doc.rust-lang.org/nomicon/poisoning.html).

#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

pub use lazy::*;
pub use once::*;
pub use rw_lock::*;

pub mod mutex;

mod lazy;
mod once;
mod rw_lock;

pub use mutex::{Mutex, MutexGuard};

/// Spin synchronisation primitives, but compatible with [`lock_api`](https://crates.io/crates/lock_api).
#[cfg(feature = "lock_api1")]
pub mod lock_api {
    /// A lock that provides mutually exclusive data access (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type Mutex<T> = lock_api::Mutex<crate::Mutex<()>, T>;

    /// A guard that provides mutable data access (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type MutexGuard<'a, T> = lock_api::MutexGuard<'a, crate::Mutex<()>, T>;

    /// A lock that provides data access to either one writer or many readers (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type RwLock<T> = lock_api::RwLock<crate::RwLock<()>, T>;

    /// A guard that provides immutable data access (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type RwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, crate::RwLock<()>, T>;

    /// A guard that provides mutable data access (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type RwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, crate::RwLock<()>, T>;

    /// A guard that provides immutable data access but can be upgraded to [`RwLockWriteGuard`] (compatible with [`lock_api`](https://crates.io/crates/lock_api)).
    pub type RwLockUpgradableReadGuard<'a, T> =
        lock_api::RwLockUpgradableReadGuard<'a, crate::RwLock<()>, T>;
}
