use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use util::cpu_relax;

/// A synchronization primitive which can be used to run a one-time global
/// initialization. Unlike its std equivalent, this is generalized so that The
/// closure returns a value and it is stored. Once therefore acts something like
/// 1a future, too.
///
/// # Examples
///
/// ```
/// #![feature(const_fn)]
/// use spin;
///
/// static START: spin::Once<()> = spin::Once::new();
///
/// START.call_once(|| {
///     // run initialization here
/// });
/// ```
pub struct Once<T> {
    state: AtomicUsize,
    data: UnsafeCell<Option<T>>, // TODO remove option and use mem::uninitialized
}

// Same unsafe impls as `std::sync::RwLock`, because this also allows for
// concurrent reads.
unsafe impl<T: Sync + Sync> Sync for Once<T> {}
unsafe impl<T: Sync + Sync> Send for Once<T> {}

// Four states that a Once can be in, encoded into the lower bits of `state` in
// the Once structure.
const INCOMPLETE: usize = 0x0;
const RUNNING: usize = 0x1;
const COMPLETE: usize = 0x2;
const PANICKED: usize = 0x3;

#[cfg(feature = "core_intrinsics")]
#[inline(always)]
fn unreachable() -> ! {
    unsafe { ::core::intrinsics::unreachable() }
}

#[cfg(not(feature = "core_intrinsics"))]
#[inline(always)]
fn unreachable() -> ! {
    unreachable!()
}

impl<T> Once<T> {
    /// Creates a new `Once` value.
    pub const fn new() -> Once<T> {
        Once {
            state: AtomicUsize::new(INCOMPLETE),
            data: UnsafeCell::new(None),
        }
    }

    fn force_get<'a>(&'a self) -> &'a T {
        match unsafe { &*self.data.get() }.as_ref() {
            None    => unreachable(),
            Some(p) => p,
        }
    }

    /// Performs an initialization routine once and only once. The given closure
    /// will be executed if this is the first time `call_once` has been called,
    /// and otherwise the routine will *not* be invoked.
    ///
    /// This method will block the calling thread if another initialization
    /// routine is currently running.
    ///
    /// When this function returns, it is guaranteed that some initialization
    /// has run and completed (it may not be the closure specified). The
    /// returned pointer points to the return value of when of those
    /// initialization closures.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(const_fn)]
    /// use spin;
    ///
    /// static INIT: spin::Once<usize> = spin::Once::new();
    ///
    /// fn get_cached_val() -> usize {
    ///     *INIT.call_once(expensive_computation)
    /// }
    ///
    /// fn expensive_computation() -> usize {
    ///     // ...
    /// # 2
    /// }
    /// ```
    pub fn call_once<'a, F>(&'a self, builder: F) -> &'a T
        where F: FnOnce() -> T
    {
        let mut status = self.state.load(Ordering::SeqCst);

        if status == INCOMPLETE {
            status = self.state.compare_and_swap(INCOMPLETE,
                                                 RUNNING,
                                                 Ordering::SeqCst);
            if status == INCOMPLETE { // We init
                // We use a guard (Finish) to catch panics caused by builder
                let mut finish = Finish { state: &self.state, panicked: true };
                unsafe { *self.data.get() = Some(builder()) };
                finish.panicked = false;

                status = COMPLETE;
                self.state.store(status, Ordering::SeqCst);

                // This next line is strictly an optomization
                return self.force_get();
            }
        }

        loop {
            match status {
                INCOMPLETE => unreachable!(),
                RUNNING => { // We spin
                    cpu_relax();
                    status = self.state.load(Ordering::SeqCst)
                },
                PANICKED => panic!("Once has panicked"),
                COMPLETE => return self.force_get(),
                _ => unreachable(),
            }
        }
    }

    /// Returns a pointer iff the `Once` was previously initialized
    pub fn try<'a>(&'a self) -> Option<&'a T> {
        match self.state.load(Ordering::SeqCst) {
            COMPLETE => Some(self.force_get()),
            _        => None,
        }
    }

    /// Like try, but will spin if the `Once` is in the process of being
    /// initialized
    pub fn wait<'a>(&'a self) -> Option<&'a T> {
        loop {
            match self.state.load(Ordering::SeqCst) {
                INCOMPLETE => return None,
                RUNNING    => cpu_relax(), // We spin
                COMPLETE   => return Some(self.force_get()),
                PANICKED   => panic!("Once has panicked"),
                _ => unreachable(),
            }
        }
    }
}

struct Finish<'a> {
    state: &'a AtomicUsize,
    panicked: bool,
}

impl<'a> Drop for Finish<'a> {
    fn drop(&mut self) {
        if self.panicked {
            self.state.store(PANICKED, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    use std::sync::mpsc::channel;
    use std::thread;
    use super::Once;

    #[test]
    fn smoke_once() {
        static O: Once<()> = Once::new();
        let mut a = 0;
        O.call_once(|| a += 1);
        assert_eq!(a, 1);
        O.call_once(|| a += 1);
        assert_eq!(a, 1);
    }

    #[test]
    fn smoke_once_value() {
        static O: Once<usize> = Once::new();
        let a = O.call_once(|| 1);
        assert_eq!(*a, 1);
        let b = O.call_once(|| 2);
        assert_eq!(*b, 1);
    }

    #[test]
    fn stampede_once() {
        static O: Once<()> = Once::new();
        static mut RUN: bool = false;

        let (tx, rx) = channel();
        for _ in 0..10 {
            let tx = tx.clone();
            thread::spawn(move|| {
                for _ in 0..4 { thread::yield_now() }
                unsafe {
                    O.call_once(|| {
                        assert!(!RUN);
                        RUN = true;
                    });
                    assert!(RUN);
                }
                tx.send(()).unwrap();
            });
        }

        unsafe {
            O.call_once(|| {
                assert!(!RUN);
                RUN = true;
            });
            assert!(RUN);
        }

        for _ in 0..10 {
            rx.recv().unwrap();
        }
    }

    #[test]
    fn try() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.try().is_none());
        INIT.call_once(|| 2);
        assert_eq!(INIT.try().map(|r| *r), Some(2));
    }

    #[test]
    fn try_no_wait() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.try().is_none());
        thread::spawn(move|| {
            INIT.call_once(|| loop { });
        });
        assert!(INIT.try().is_none());
    }


    #[test]
    fn wait() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.wait().is_none());
        INIT.call_once(|| 3);
        assert_eq!(INIT.wait().map(|r| *r), Some(3));
    }

    #[test]
    fn panic() {
        use ::std::panic;

        static INIT: Once<()> = Once::new();

        // poison the once
        let t = panic::catch_unwind(|| {
            INIT.call_once(|| panic!());
        });
        assert!(t.is_err());

        // poisoning propagates
        let t = panic::catch_unwind(|| {
            INIT.call_once(|| {});
        });
        assert!(t.is_err());
    }
}
