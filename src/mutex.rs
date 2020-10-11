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

use core::{
    fmt,
    ops::{Deref, DerefMut},
};

/// Macro for choosing one of two expressions,
/// based on the feature flag `ticket_mutex`.
macro_rules! helper {
    ($one:expr, $two:expr) => {{
        #[cfg(feature = "ticket_mutex")]
        let inner = $one;
        #[cfg(not(feature = "ticket_mutex"))]
        let inner = $two;

        inner
    }};
}

/// A generic `Mutex`, that will use either a fully spin-based mutex,
/// or a ticket lock, depending on the `ticket_mutex` feature, to guarantee mutual exclusion.
///
/// For more info see [`TicketMutex`] or [`SpinMutex`].
///
/// [`TicketMutex`]: ./struct.TicketMutex.html
/// [`SpinMutex`]: ./struct.SpinMutex.html
pub struct Mutex<T: ?Sized> {
    #[cfg(feature = "ticket_mutex")]
    inner: TicketMutex<T>,
    #[cfg(not(feature = "ticket_mutex"))]
    inner: SpinMutex<T>,
}

unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}

/// A generic guard that will protect some data access and
/// uses either a ticket lock or a normal spin mutex.
///
/// For more info see [`TicketMutexGuard`] or [`SpinMutexGuard`].
///
/// [`TicketMutexGuard`]: ./struct.TicketMutexGuard.html
/// [`SpinMutexGuard`]: ./struct.SpinMutexGuard.html
pub struct MutexGuard<'a, T: 'a + ?Sized> {
    #[cfg(feature = "ticket_mutex")]
    inner: TicketMutexGuard<'a, T>,
    #[cfg(not(feature = "ticket_mutex"))]
    inner: SpinMutexGuard<'a, T>,
}

impl<T> Mutex<T> {
    /// Creates a new `Mutex`.
    ///
    /// This method can be used in a static context.
    ///
    /// See [`TicketMutex::new`] or [`SpinMutex::new`].
    ///
    /// [`TicketMutex::new`]: ./struct.TicketMutex.html#method.new
    /// [`SpinMutex::new`]: ./struct.SpinMutex.html#method.new
    pub const fn new(value: T) -> Self {
        let inner = helper!(TicketMutex::new(value), SpinMutex::new(value));
        Self { inner }
    }

    /// Consumes this `Mutex` and unwraps the underlying data.
    ///
    /// See [`TicketMutex::into_inner`] or [`SpinMutex::into_inner`].
    ///
    /// [`TicketMutex::into_inner`]: ./struct.TicketMutex.html#method.into_inner
    /// [`SpinMutex::into_inner`]: ./struct.SpinMutex.html#method.into_inner
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Locks the spinlock and returns a guard.
    ///
    /// See [`TicketMutex::lock`] or [`SpinMutex::lock`].
    ///
    /// [`TicketMutex::lock`]: ./struct.TicketMutex.html#method.lock
    /// [`SpinMutex::lock`]: ./struct.SpinMutex.html#method.lock
    pub fn lock(&self) -> MutexGuard<T> {
        MutexGuard {
            inner: self.inner.lock(),
        }
    }

    /// Force unlock the spinlock.
    ///
    /// # Safety
    ///
    /// This is *extremely* unsafe if the lock is not held by the current
    /// thread. However, this can be useful in some instances for exposing the
    /// lock to FFI that doesn't know how to deal with RAII.
    ///
    /// See [`TicketMutex::force_unlock`] or [`SpinMutex::force_unlock`].
    ///
    /// [`TicketMutex::force_unlock`]: ./struct.TicketMutex.html#method.force_unlock
    /// [`SpinMutex::force_unlock`]: ./struct.SpinMutex.html#method.force_unlock
    pub unsafe fn force_unlock(&self) {
        self.inner.force_unlock()
    }

    /// Tries to lock the mutex. If it is already locked, it will return None. Otherwise it returns
    /// a guard within Some.
    ///
    /// See [`TicketMutex::try_lock`] or [`SpinMutex::try_lock`].
    ///
    /// [`TicketMutex::try_lock`]: ./struct.TicketMutex.html#method.try_lock
    /// [`SpinMutex::try_lock`]: ./struct.SpinMutex.html#method.try_lock
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        self.inner
            .try_lock()
            .map(|guard| MutexGuard { inner: guard })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the `Mutex` mutably, no actual locking needs to
    /// take place -- the mutable borrow statically guarantees no locks exist.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl<T: ?Sized + Default> Default for Mutex<T> {
    fn default() -> Mutex<T> {
        Self::new(Default::default())
    }
}

impl<T> From<T> for Mutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<'a, T: ?Sized> MutexGuard<'a, T> {
    /// Leak the lock guard, yielding a mutable reference to the underlying data.
    ///
    /// Note that this function will permanently lock the original lock.
    ///
    /// See [`TicketMutexGuard::leak`] or [`SpinMutexGuard::leak`].
    ///
    /// [`TicketMutexGuard::leak`]: ./struct.TicketMutex.html#method.leak
    /// [`SpinMutexGuard::leak`]: ./struct.SpinMutex.html#method.leak
    #[inline]
    pub fn leak(this: Self) -> &'a mut T {
        helper!(
            TicketMutexGuard::leak(this.inner),
            SpinMutexGuard::leak(this.inner)
        )
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T: ?Sized> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

#[cfg(feature = "lock_api1")]
unsafe impl lock_api::RawMutex for Mutex<()> {
    type GuardMarker = lock_api::GuardSend;

    const INIT: Self = Self::new(());

    fn lock(&self) {
        // Prevent guard destructor running
        core::mem::forget(Self::lock(self));
    }

    fn try_lock(&self) -> bool {
        // Prevent guard destructor running
        Self::try_lock(self).map(core::mem::forget).is_some()
    }

    unsafe fn unlock(&self) {
        self.force_unlock();
    }

    fn is_locked(&self) -> bool {
        use core::sync::atomic::Ordering;

        helper!(
            {
                let this = &self.inner;
                let ticket = this.next_ticket.load(Ordering::Acquire) + 1;
                this.next_serving.load(Ordering::Acquire) != ticket
            },
            self.inner.lock.load(Ordering::Relaxed)
        )
    }
}
