//! Implementation of two different Mutex versions, [`TicketMutex`] and [`SpinMutex`].
//!
//! The [`Mutex`] in the root of the crate, can be configured using the `ticket_mutex` feature.
//! If it's enabled, [`TicketMutex`] and [`TicketMutexGuard`] will be re-exported as [`Mutex`]
//! and [`MutexGuard`], otherwise the [`SpinMutex`] and guard will be re-exported.
//!
//! `ticket_mutex` is enabled by default.
//!
//! [`Mutex`]: ../struct.Mutex.html
//! [`MutexGuard`]: ../struct.MutexGuard.html
//! [`TicketMutex`]: ./struct.TicketMutex.html
//! [`TicketMutexGuard`]: ./struct.TicketMutexGuard.html
//! [`SpinMutex`]: ./struct.SpinMutex.html

mod spin;
pub use self::spin::*;

mod ticket;
pub use self::ticket::*;
