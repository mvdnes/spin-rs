use critical_section::RestoreState;
use critical_section::acquire;
use critical_section::release;

use core::{
    fmt,
    ops::{Deref, DerefMut},
};

type InnerMutex<T> = crate::Mutex<T>;
type InnerMutexGuard<'a, T> = crate::MutexGuard<'a, T>;

/// A spin-based lock providing mutually exclusive access to data.
///
/// The implementation uses either a ticket mutex or a regular spin mutex depending on whether the `spin_mutex` or
/// `ticket_mutex` feature flag is enabled.
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
/// # let mut ts = Vec::new();
/// for _ in 0..thread_count {
///     let my_barrier = barrier.clone();
///     let my_lock = spin_mutex.clone();
/// # let t =
///     std::thread::spawn(move || {
///         let mut guard = my_lock.lock();
///         *guard += 1;
///
///         // Release the lock to prevent a deadlock
///         drop(guard);
///         my_barrier.wait();
///     });
/// # ts.push(t);
/// }
///
/// barrier.wait();
///
/// let answer = { *spin_mutex.lock() };
/// assert_eq!(answer, thread_count);
///
/// # for t in ts {
/// #     t.join().unwrap();
/// # }
/// ```
pub struct IrqMutex<T: ?Sized> {
    inner: InnerMutex<T>
}

/// A generic guard that will protect some data access and
/// uses either a ticket lock or a normal spin mutex.
///
/// For more info see [`TicketMutexGuard`] or [`SpinMutexGuard`].
///
/// [`TicketMutexGuard`]: ./struct.TicketMutexGuard.html
/// [`SpinMutexGuard`]: ./struct.SpinMutexGuard.html
pub struct IrqMutexGuard<'a, T: 'a + ?Sized> {
    inner: InnerMutexGuard<'a, T>,
    irq_state: RestoreState
}

impl<T> IrqMutex<T> {
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
    /// 
    ///
    /// 
    /// ```
    #[inline(always)]
    pub const fn new(value: T) -> Self {
        Self {
            inner: InnerMutex::new(value),
        }
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

impl<T: ?Sized> IrqMutex<T> {
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
    /// UNSAFE:
    /// When IrqMutex's are nested, the innter IrqMutexGuard must not outlive the outer IrqMutexGuard, a violation leads to Undefinded Interrupt Behavior
    #[inline(always)]
    pub unsafe fn lock(&self) -> IrqMutexGuard<T> {
        let state = unsafe{acquire()};
        IrqMutexGuard {
            inner: self.inner.lock(),
            irq_state: state
        }
    }
}

impl<T: ?Sized> IrqMutex<T> {
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
    pub fn try_lock(&self) -> Option<IrqMutexGuard<T>> {
        let state = unsafe{acquire()};
        let maybe_guard = self.inner
            .try_lock()
            .map(|guard| IrqMutexGuard { inner: guard, irq_state: state });
        if maybe_guard.is_none() {
            unsafe{release(state)};
        }
        maybe_guard
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

impl<T: ?Sized + fmt::Debug> fmt::Debug for IrqMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}

impl<T: ?Sized + Default> Default for IrqMutex<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl<T> From<T> for IrqMutex<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<'a, T: ?Sized> IrqMutexGuard<'a, T> {
    /// Leak the lock guard, yielding a mutable reference to the underlying data.
    ///
    /// Note that this function will permanently lock the original [`Mutex`].
    /// Restores the Interrupt State
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
        unsafe { release(this.irq_state) }
        InnerMutexGuard::leak(this.inner)
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for IrqMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Display> fmt::Display for IrqMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for IrqMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T: ?Sized> DerefMut for IrqMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}

impl<'a, T: ?Sized> Drop for IrqMutexGuard<'a, T> {
    /// The dropping of the IrqMutexGuard will release the lock it was created from and restore Interrupts to its former value
    fn drop(&mut self) {
        unsafe{release(self.irq_state)};
    }
}
