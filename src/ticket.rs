use core::{
    cell::UnsafeCell,
    fmt,
    ops::{Deref, DerefMut},
    sync::atomic::{spin_loop_hint, AtomicUsize, Ordering},
};

/// A ticket lock for mutual exclusion based on spinning.
///
/// A ticket lock is similair to a queue management system.
/// When a thread tries to lock the values,
/// it is assigned a ticket, and it spins until its ticket is the next in line.
/// When releasing the value, the next ticket will be processed.
pub struct TicketLock<T: ?Sized> {
    next_ticket: AtomicUsize,
    next_serving: AtomicUsize,
    value: UnsafeCell<T>,
}

/// A guard that protects some data.
///
/// When the guard is dropped, the next ticket will be processed.
pub struct LockGuard<'a, T: ?Sized + 'a> {
    next_serving: &'a AtomicUsize,
    ticket: usize,
    value: &'a mut T,
}

unsafe impl<T: ?Sized + Send> Sync for TicketLock<T> {}
unsafe impl<T: ?Sized + Send> Send for TicketLock<T> {}

impl<T> TicketLock<T> {
    /// Creates a new `TicketLock` that wraps the given value.
    ///
    /// This method can be used in static context.
    ///
    /// ```
    /// static LOCK: spin::TicketLock<i32> = spin::TicketLock::new(123);
    ///
    /// # fn main() {
    /// let value = LOCK.lock();
    /// assert_eq!(value, 123);
    /// drop(value);
    /// #}
    /// ```
    pub const fn new(value: T) -> Self {
        Self {
            next_ticket: AtomicUsize::new(0),
            next_serving: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    /// Unwraps the inner value of this lock.
    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T: ?Sized> TicketLock<T> {
    /// Spins the thread until the value can be locked.
    ///
    /// The returned value can be used to modify the value,
    /// and will be unlocked after dropping the return value.
    pub fn lock(&self) -> LockGuard<T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);

        while self.next_serving.load(Ordering::Relaxed) != ticket {
            spin_loop_hint();
        }

        LockGuard {
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

    /// Tries to lock this lock. If it's already locked, `None` is returned,
    /// otherwise a guard that protects the data is returned.
    pub fn try_lock(&self) -> Option<LockGuard<T>> {
        let ticket = self
            .next_ticket
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |ticket| {
                if self.next_serving.load(Ordering::Acquire) == ticket {
                    Some(ticket + 1)
                } else {
                    None
                }
            });

        ticket.ok().map(|ticket| LockGuard {
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
    /// This will not lock this lock, since this method borrows
    /// this lock mutably so no locking is needed.
    pub fn get_mut(&mut self) -> &mut T {
        // Safety:
        // We know that there are no other references to `self`,
        // so it's safe to return a exclusive reference to the value.
        unsafe { &mut *self.value.get() }
    }
}

impl<T: ?Sized + Default> Default for TicketLock<T> {
    fn default() -> TicketLock<T> {
        TicketLock::new(Default::default())
    }
}

impl<'a, T: ?Sized + fmt::Debug> fmt::Debug for LockGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized + fmt::Display> fmt::Display for LockGuard<'a, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<'a, T: ?Sized> Deref for LockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}

impl<'a, T: ?Sized> DerefMut for LockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value
    }
}

impl<'a, T: ?Sized> Drop for LockGuard<'a, T> {
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
        let m = TicketLock::new(());
        drop(m.lock());
        drop(m.lock());
    }

    #[test]
    fn lots_and_lots() {
        static M: TicketLock<()> = TicketLock::new(());
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
        let mutex = TicketLock::new(42);

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
        let m = TicketLock::new(NonCopy(10));
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
        let m = TicketLock::new(Foo(num_drops.clone()));
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
        let arc = Arc::new(TicketLock::new(1));
        let arc2 = Arc::new(TicketLock::new(arc));
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
        let arc = Arc::new(TicketLock::new(1));
        let arc2 = arc.clone();
        let _ = thread::spawn(move || -> () {
            struct Unwinder {
                i: Arc<TicketLock<i32>>,
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
        let mutex: &TicketLock<[i32]> = &TicketLock::new([1, 2, 3]);
        {
            let b = &mut *mutex.lock();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*mutex.lock(), comp);
    }
}
