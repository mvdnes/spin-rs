//! A condition variable based on spinning.

use {
    crate::{Mutex, MutexGuard},
    core::{
        mem, ptr,
        sync::atomic::{AtomicBool, Ordering},
    },
};

/// A condition variable based on spinning.
pub struct Condvar {
    head: Mutex<WaiterPtr>,
}

impl Condvar {
    /// Create a new condition variable.
    pub const fn new() -> Self {
        Self {
            head: Mutex::new(WaiterPtr(ptr::null())),
        }
    }

    /// Notify at most one waiter. Returns `true` if a waiter was notified, and
    /// `false` otherwise.
    pub fn notify_one(&self) -> bool {
        let mut head = self.head.lock();
        Self::notify_one_locked(&mut head)
    }

    /// Notify all waiters. Returns the number of waiters that were notified.
    pub fn notify_all(&self) -> usize {
        let mut head = self.head.lock();
        let mut count = 0;
        while Self::notify_one_locked(&mut head) {
            count += 1;
        }
        count
    }

    /// Atomically drop `guard` and start waiting until notified. Upon being
    /// notified, re-acquire a new `MutexGuard` and return it.
    pub fn wait<'a, T>(&self, mutex: &'a Mutex<T>, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T>
    where
        T: 'a + ?Sized,
    {
        let mut head = self.head.lock();
        let waiter = Waiter {
            next: *head,
            wait: AtomicBool::new(true),
        };
        *head = WaiterPtr(&waiter);
        mem::drop(head);

        mem::drop(guard);

        while waiter.wait.load(Ordering::SeqCst) {
            crate::relax();
        }

        mutex.lock()
    }

    fn notify_one_locked(head: &mut WaiterPtr) -> bool {
        if head.0.is_null() {
            false
        } else {
            let w = head.0;

            // SAFETY: `w` is guaranteed not to be dangling because waiter is
            // spinning on `w.wait`.
            *head = unsafe { &*w }.next;

            // SAFETY: `w` is guaranteed not to be dangling because waiter is
            // spinning on `w.wait`.
            unsafe { &*w }.wait.store(false, Ordering::SeqCst);
            // NOTE: After the store above completes, `w` may be dangling.

            true
        }
    }
}

#[derive(Clone, Copy)]
struct WaiterPtr(*const Waiter);

unsafe impl Send for WaiterPtr {}

struct Waiter {
    next: WaiterPtr,
    wait: AtomicBool,
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    use std::sync::Arc;
    use std::thread;

    use super::*;

    #[test]
    fn smoke() {
        let m = Arc::new(Mutex::new(()));
        let c = Arc::new(Condvar::new());
        let t = {
            let c = c.clone();
            thread::spawn(move || {
                c.notify_one();
            })
        };
        let g = m.lock();
        c.wait(&m, g);
        t.join().expect("Failed to join.");
    }

    #[test]
    fn wait_for_condition() {
        let m = Arc::new(Mutex::new(0i32));
        let c = Arc::new(Condvar::new());
        let t = {
            let m = m.clone();
            let c = c.clone();
            thread::spawn(move || {
                for _ in 0..5 {
                    {
                        let mut g = m.lock();
                        *g += 1;
                    }
                    c.notify_one();
                }
            })
        };
        let mut g = m.lock();
        while *g != 5 {
            g = c.wait(&*m, g);
        }
        t.join().expect("Failed to join.");
    }
}
