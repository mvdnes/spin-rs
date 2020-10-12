//! Locks that have the same behaviour as a mutex.
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

#[cfg(feature = "ticket_mutex")]
type InnerMutex<T> = TicketMutex<T>;
#[cfg(feature = "ticket_mutex")]
type InnerMutexGuard<'a, T> = TicketMutexGuard<'a, T>;

#[cfg(not(feature = "ticket_mutex"))]
type InnerMutex<T> = SpinMutex<T>;
#[cfg(not(feature = "ticket_mutex"))]
type InnerMutexGuard<'a, T> = SpinMutexGuard<'a, T>;

/// A spin-based lock providing mutually exclusive access to data.
///
/// The implementation uses either a [`TicketMutex`] or a regular [`SpinMutex`] depending on whether the `ticket_mutex`
/// feature flag is enabled.
///
/// # Example
///
/// ```
/// use spin;
///
/// let lock = spin::Mutex::new(0);
///
/// // Modify the data
/// *lock.lock() = 2;
///
/// // Read the data
/// let answer = *lock.lock();
/// assert_eq!(answer, 2);
/// ```
///
/// # Thread safety example
///
/// ```
/// use spin;
/// use std::sync::{Arc, Barrier};
///
/// let thread_count = 1000;
/// let spin_mutex = Arc::new(spin::Mutex::new(0));
///
/// // We use a barrier to ensure the readout happens after all writing
/// let barrier = Arc::new(Barrier::new(thread_count + 1));
///
/// for _ in (0..thread_count) {
///     let my_barrier = barrier.clone();
///     let my_lock = spin_mutex.clone();
///     std::thread::spawn(move || {
///         let mut guard = my_lock.lock();
///         *guard += 1;
///
///         // Release the lock to prevent a deadlock
///         drop(guard);
///         my_barrier.wait();
///     });
/// }
///
/// barrier.wait();
///
/// let answer = { *spin_mutex.lock() };
/// assert_eq!(answer, thread_count);
/// ```
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
    /// Creates a new [`Mutex`] wrapping the supplied data.
    ///
    /// # Example
    ///
    /// ```
    /// use spin::Mutex;
    ///
    /// static MUTEX: Mutex<()> = Mutex::new(());
    ///
    /// fn demo() {
    ///     let lock = MUTEX.lock();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline(always)]
    pub const fn new(value: T) -> Self {
        Self { inner: InnerMutex::new(value) }
    }

    /// Consumes this [`Mutex`] and unwraps the underlying data.
    ///
    /// # Example
    ///
    /// ```
    /// let lock = spin::Mutex::new(42);
    /// assert_eq!(42, lock.into_inner());
    /// ```
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Returns `true` if the lock is currently held.
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }

    /// Locks the [`Mutex`] and returns a guard that permits access to the inner data.
    ///
    /// The returned value may be dereferenced for data access
    /// and the lock will be dropped when the guard falls out of scope.
    ///
    /// ```
    /// let lock = spin::Mutex::new(0);
    /// {
    ///     let mut data = lock.lock();
    ///     // The lock is now locked and the data can be accessed
    ///     *data += 1;
    ///     // The lock is implicitly dropped at the end of the scope
    /// }
    /// ```
    #[inline(always)]
    pub fn lock(&self) -> MutexGuard<T> {
        MutexGuard {
            inner: self.inner.lock(),
        }
    }

    /// Force unlock this [`Mutex`].
    ///
    /// # Safety
    ///
    /// This is *extremely* unsafe if the lock is not held by the current
    /// thread. However, this can be useful in some instances for exposing the
    /// lock to FFI that doesn't know how to deal with RAII.
    #[inline(always)]
    pub unsafe fn force_unlock(&self) {
        self.inner.force_unlock()
    }

    /// Try to lock this [`Mutex`], returning a lock guard if successful.
    ///
    /// # Example
    ///
    /// ```
    /// let lock = spin::Mutex::new(42);
    ///
    /// let maybe_guard = lock.try_lock();
    /// assert!(maybe_guard.is_some());
    ///
    /// // `maybe_guard` is still held, so the second call fails
    /// let maybe_guard2 = lock.try_lock();
    /// assert!(maybe_guard2.is_none());
    /// ```
    #[inline(always)]
    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        self.inner
            .try_lock()
            .map(|guard| MutexGuard { inner: guard })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the [`Mutex`] mutably, and a mutable reference is guaranteed to be exclusive in Rust,
    /// no actual locking needs to take place -- the mutable borrow statically guarantees no locks exist. As such,
    /// this is a 'zero-cost' operation.
    ///
    /// # Example
    ///
    /// ```
    /// let mut lock = spin::Mutex::new(0);
    /// *lock.get_mut() = 10;
    /// assert_eq!(*lock.lock(), 10);
    /// ```
    #[inline(always)]
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
    /// Note that this function will permanently lock the original [`Mutex`].
    ///
    /// ```
    /// let mylock = spin::Mutex::new(0);
    ///
    /// let data: &mut i32 = spin::MutexGuard::leak(mylock.lock());
    ///
    /// *data = 1;
    /// assert_eq!(*data, 1);
    /// ```
    #[inline(always)]
    pub fn leak(this: Self) -> &'a mut T {
        InnerMutexGuard::leak(this.inner)
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
        self.inner.is_locked()
    }
}
