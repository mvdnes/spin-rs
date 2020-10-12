use core::{
    cell::UnsafeCell,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicUsize, Ordering},
};

/// A spin-based [ticket lock](https://en.wikipedia.org/wiki/Ticket_lock) providing mutually exclusive access to data.
///
/// A ticket lock is analagous to a queue management system for lock requests. When a thread tries to take a lock, it
/// is assigned a 'ticket'. It then spins until its ticket becomes next in line. When the lock guard is released, the
/// next ticket will be processed.
///
/// Ticket locks significantly reduce the worse-case performance of locking at the cost of slightly higher average-time
/// overhead.
///
/// # Example
///
/// ```
/// use spin;
///
/// let lock = spin::mutex::TicketMutex::new(0);
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
/// let spin_mutex = Arc::new(spin::mutex::TicketMutex::new(0));
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
pub struct TicketMutex<T: ?Sized> {
    pub(crate) next_ticket: AtomicUsize,
    pub(crate) next_serving: AtomicUsize,
    value: UnsafeCell<T>,
}

/// A guard that protects some data.
///
/// When the guard is dropped, the next ticket will be processed.
pub struct TicketMutexGuard<'a, T: ?Sized + 'a> {
    next_serving: &'a AtomicUsize,
    ticket: usize,
    value: &'a mut T,
}

unsafe impl<T: ?Sized + Send> Sync for TicketMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for TicketMutex<T> {}

impl<T> TicketMutex<T> {
    /// Creates a new [`TicketMutex`] wrapping the supplied data.
    ///
    /// # Example
    ///
    /// ```
    /// use spin::mutex::TicketMutex;
    ///
    /// static MUTEX: TicketMutex<()> = TicketMutex::new(());
    ///
    /// fn demo() {
    ///     let lock = MUTEX.lock();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline(always)]
    pub const fn new(value: T) -> Self {
        Self {
            next_ticket: AtomicUsize::new(0),
            next_serving: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Consumes this [`TicketMutex`] and unwraps the underlying data.
    ///
    /// # Example
    ///
    /// ```
    /// let lock = spin::mutex::TicketMutex::new(42);
    /// assert_eq!(42, lock.into_inner());
    /// ```
    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for TicketMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => write!(f, "Mutex {{ data: ")
                .and_then(|()| (&*guard).fmt(f))
                .and_then(|()| write!(f, "}}")),
            None => write!(f, "Mutex {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized> TicketMutex<T> {
    /// Returns `true` if the lock is currently held.
    ///
    /// # Safety
    ///
    /// This function provides no synchronization guarantees and so its result should be considered 'out of date'
    /// the instant it is called. Do not use it for synchronization purposes. However, it may be useful as a heuristic.
    #[inline(always)]
    pub fn is_locked(&self) -> bool {
        let ticket = self.next_ticket.load(Ordering::Relaxed);
        self.next_serving.load(Ordering::Relaxed) != ticket
    }

    /// Locks the [`TicketMutex`] and returns a guard that permits access to the inner data.
    ///
    /// The returned value may be dereferenced for data access
    /// and the lock will be dropped when the guard falls out of scope.
    ///
    /// ```
    /// let lock = spin::mutex::TicketMutex::new(0);
    /// {
    ///     let mut data = lock.lock();
    ///     // The lock is now locked and the data can be accessed
    ///     *data += 1;
    ///     // The lock is implicitly dropped at the end of the scope
    /// }
    /// ```
    #[inline(always)]
    pub fn lock(&self) -> TicketMutexGuard<T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);

        while self.next_serving.load(Ordering::Acquire) != ticket {
            crate::relax();
        }

        TicketMutexGuard {
            next_serving: &self.next_serving,
            ticket,
            // Safety
            // We know that we are the next ticket to be served,
            // so there's no other thread accessing the value.
            //
            // Every other thread has another ticket number so it's
            // definitely stuck in the spin loop above.
            value: unsafe { &mut *self.value.get() },
        }
    }

    /// Force unlock this [`TicketMutex`], by serving the next ticket.
    ///
    /// # Safety
    ///
    /// This is *extremely* unsafe if the lock is not held by the current
    /// thread. However, this can be useful in some instances for exposing the
    /// lock to FFI that doesn't know how to deal with RAII.
    #[inline(always)]
    pub unsafe fn force_unlock(&self) {
        self.next_serving.fetch_add(1, Ordering::Release);
    }

    /// Try to lock this [`TicketMutex`], returning a lock guard if successful.
    ///
    /// # Example
    ///
    /// ```
    /// let lock = spin::mutex::TicketMutex::new(42);
    ///
    /// let maybe_guard = lock.try_lock();
    /// assert!(maybe_guard.is_some());
    ///
    /// // `maybe_guard` is still held, so the second call fails
    /// let maybe_guard2 = lock.try_lock();
    /// assert!(maybe_guard2.is_none());
    /// ```
    #[inline(always)]
    pub fn try_lock(&self) -> Option<TicketMutexGuard<T>> {
        let ticket = self
            .next_ticket
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |ticket| {
                if self.next_serving.load(Ordering::Acquire) == ticket {
                    Some(ticket + 1)
                } else {
                    None
                }
            });

        ticket.ok().map(|ticket| TicketMutexGuard {
            next_serving: &self.next_serving,
            ticket,
            // Safety
            // We have a ticket that is equal to the next_serving ticket, so we know:
            // - that no other thread can have the same ticket id as this thread
            // - that we are the next one to be served so we have exclusive access to the value
            value: unsafe { &mut *self.value.get() },
        })
    }

    /// Returns a mutable reference to the underlying data.
    ///
    /// Since this call borrows the [`TicketMutex`] mutably, and a mutable reference is guaranteed to be exclusive in
    /// Rust, no actual locking needs to take place -- the mutable borrow statically guarantees no locks exist. As
    /// such, this is a 'zero-cost' operation.
    ///
    /// # Example
    ///
    /// ```
    /// let mut lock = spin::mutex::TicketMutex::new(0);
    /// *lock.get_mut() = 10;
    /// assert_eq!(*lock.lock(), 10);
    /// ```
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        // Safety:
        // We know that there are no other references to `self`,
        // so it's safe to return a exclusive reference to the value.
        unsafe { &mut *self.value.get() }
    }
}

impl<T: ?Sized + Default> Default for TicketMutex<T> {
    fn default() -> TicketMutex<T> {
        TicketMutex::new(Default::default())
    }
}

impl<T> From<T> for TicketMutex<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<'a, T: ?Sized> TicketMutexGuard<'a, T> {
    /// Leak the lock guard, yielding a mutable reference to the underlying data.
    ///
    /// Note that this function will permanently lock the original [`TicketMutex`].
    ///
    /// ```
    /// let mylock = spin::mutex::TicketMutex::new(0);
    ///
    /// let data: &mut i32 = spin::mutex::TicketMutexGuard::leak(mylock.lock());
    ///
    /// *data = 1;
    /// assert_eq!(*data, 1);
    /// ```
    #[inline(always)]
    pub fn leak(this: Self) -> &'a mut T {
        let data = this.value as *mut _; // Keep it in pointer form temporarily to avoid double-aliasing
        core::mem::forget(this);
        unsafe { &mut *data }
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for TicketMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Display> fmt::Display for TicketMutexGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for TicketMutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T: ?Sized> DerefMut for TicketMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T: ?Sized> Drop for TicketMutexGuard<'a, T> {
    fn drop(&mut self) {
        let new_ticket = self.ticket + 1;
        self.next_serving.store(new_ticket, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct NonCopy(i32);

    #[test]
    fn smoke() {
        let m = TicketMutex::new(());
        drop(m.lock());
        drop(m.lock());
    }

    #[test]
    fn lots_and_lots() {
        static M: TicketMutex<()> = TicketMutex::new(());
        static mut CNT: u32 = 0;
        const J: u32 = 1000;
        const K: u32 = 3;

        fn inc() {
            for _ in 0..J {
                unsafe {
                    let _g = M.lock();
                    CNT += 1;
                }
            }
        }

        let (tx, rx) = channel();
        for _ in 0..K {
            let tx2 = tx.clone();
            thread::spawn(move || {
                inc();
                tx2.send(()).unwrap();
            });
            let tx2 = tx.clone();
            thread::spawn(move || {
                inc();
                tx2.send(()).unwrap();
            });
        }

        drop(tx);
        for _ in 0..2 * K {
            rx.recv().unwrap();
        }
        assert_eq!(unsafe { CNT }, J * K * 2);
    }

    #[test]
    fn try_lock() {
        let mutex = TicketMutex::new(42);

        // First lock succeeds
        let a = mutex.try_lock();
        assert_eq!(a.as_ref().map(|r| **r), Some(42));

        // Additional lock failes
        let b = mutex.try_lock();
        assert!(b.is_none());

        // After dropping lock, it succeeds again
        ::core::mem::drop(a);
        let c = mutex.try_lock();
        assert_eq!(c.as_ref().map(|r| **r), Some(42));
    }

    #[test]
    fn test_into_inner() {
        let m = TicketMutex::new(NonCopy(10));
        assert_eq!(m.into_inner(), NonCopy(10));
    }

    #[test]
    fn test_into_inner_drop() {
        struct Foo(Arc<AtomicUsize>);
        impl Drop for Foo {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let num_drops = Arc::new(AtomicUsize::new(0));
        let m = TicketMutex::new(Foo(num_drops.clone()));
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        {
            let _inner = m.into_inner();
            assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        }
        assert_eq!(num_drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_mutex_arc_nested() {
        // Tests nested mutexes and access
        // to underlying data.
        let arc = Arc::new(TicketMutex::new(1));
        let arc2 = Arc::new(TicketMutex::new(arc));
        let (tx, rx) = channel();
        let _t = thread::spawn(move || {
            let lock = arc2.lock();
            let lock2 = lock.lock();
            assert_eq!(*lock2, 1);
            tx.send(()).unwrap();
        });
        rx.recv().unwrap();
    }

    #[test]
    fn test_mutex_arc_access_in_unwind() {
        let arc = Arc::new(TicketMutex::new(1));
        let arc2 = arc.clone();
        let _ = thread::spawn(move || -> () {
            struct Unwinder {
                i: Arc<TicketMutex<i32>>,
            }
            impl Drop for Unwinder {
                fn drop(&mut self) {
                    *self.i.lock() += 1;
                }
            }
            let _u = Unwinder { i: arc2 };
            panic!();
        })
        .join();
        let lock = arc.lock();
        assert_eq!(*lock, 2);
    }

    #[test]
    fn test_mutex_unsized() {
        let mutex: &TicketMutex<[i32]> = &TicketMutex::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock(), comp);
    }

    #[test]
    fn is_locked() {
        let mutex = TicketMutex::new(());
        assert!(!mutex.is_locked());
        let lock = mutex.lock();
        assert!(mutex.is_locked());
        drop(lock);
        assert!(!mutex.is_locked());
    }
}
