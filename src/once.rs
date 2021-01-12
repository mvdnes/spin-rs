//! Synchronization primitives for one-time evaluation.

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
    fmt,
};

/// A primitive that provides lazy one-time initialization.
///
/// Unlike its `std::sync` equivalent, this is generalized such that the closure returns a
/// value to be stored by the [`Once`] (`std::sync::Once` can be trivially emulated with
/// `Once<()>`).
///
/// Because [`Once::new`] is `const`, this primitive may be used to safely initialize statics.
///
/// # Examples
///
/// ```
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
    data: UnsafeCell<MaybeUninit<T>>,
}

impl<T: fmt::Debug> fmt::Debug for Once<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.get() {
            Some(s) => write!(f, "Once {{ data: ")
				.and_then(|()| s.fmt(f))
				.and_then(|()| write!(f, "}}")),
            None => write!(f, "Once {{ <uninitialized> }}")
        }
    }
}

// Same unsafe impls as `std::sync::RwLock`, because this also allows for
// concurrent reads.
unsafe impl<T: Send + Sync> Sync for Once<T> {}
unsafe impl<T: Send> Send for Once<T> {}

// Four states that a Once can be in, encoded into the lower bits of `state` in
// the Once structure.
const INCOMPLETE: usize = 0x0;
const RUNNING: usize = 0x1;
const COMPLETE: usize = 0x2;
const PANICKED: usize = 0x3;

use core::hint::unreachable_unchecked as unreachable;

impl<T> Once<T> {
    /// Initialization constant of [`Once`].
    #[allow(clippy::declare_interior_mutable_const)]
    pub const INIT: Self = Self {
        state: AtomicUsize::new(INCOMPLETE),
        data: UnsafeCell::new(MaybeUninit::uninit()),
    };

    /// Creates a new [`Once`].
    pub const fn new() -> Once<T> {
        Self::INIT
    }

    /// Creates a new initialized [`Once`].
    pub const fn initialized(data: T) -> Once<T> {
        Self {
            state: AtomicUsize::new(COMPLETE),
            data: UnsafeCell::new(MaybeUninit::new(data)),
        }
    }

    /// Get a reference to the initialized instance. Must only be called once COMPLETE.
    unsafe fn force_get(&self) -> &T {
        // SAFETY:
        // * `UnsafeCell`/inner deref: data never changes again
        // * `MaybeUninit`/outer deref: data was initialized
        &*(*self.data.get()).as_ptr()
    }

    /// Get a reference to the initialized instance. Must only be called once COMPLETE.
    unsafe fn force_get_mut(&mut self) -> &mut T {
        // SAFETY:
        // * `UnsafeCell`/inner deref: data never changes again
        // * `MaybeUninit`/outer deref: data was initialized
        &mut *(*self.data.get()).as_mut_ptr()
    }

    /// Get a reference to the initialized instance. Must only be called once COMPLETE.
    unsafe fn force_into_inner(self) -> T {
        // SAFETY:
        // * `UnsafeCell`/inner deref: data never changes again
        // * `MaybeUninit`/outer deref: data was initialized
        (*self.data.get()).as_ptr().read()
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
    /// returned pointer will point to the result from the closure that was
    /// run.
    ///
    /// # Panics
    ///
    /// This function will panic if the [`Once`] previously panicked while attempting
    /// to initialize. This is similar to the poisoning behaviour of `std::sync`'s
    /// primitives.
    ///
    /// # Examples
    ///
    /// ```
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
    pub fn call_once<F: FnOnce() -> T>(&self, f: F) -> &T {
        let mut status = self.state.load(Ordering::SeqCst);

        if status == INCOMPLETE {
            status = self.state.compare_and_swap(
                INCOMPLETE,
                RUNNING,
                Ordering::SeqCst,
            );

            if status == INCOMPLETE { // We init
                // We use a guard (Finish) to catch panics caused by builder
                let mut finish = Finish { state: &self.state, panicked: true };
                unsafe {
                    // SAFETY:
                    // `UnsafeCell`/deref: currently the only accessor, mutably
                    // and immutably by cas exclusion.
                    // `write`: pointer comes from `MaybeUninit`.
                    (*self.data.get()).as_mut_ptr().write(f())
                };
                finish.panicked = false;

                status = COMPLETE;
                self.state.store(status, Ordering::SeqCst);

                // This next line is strictly an optimization
                return unsafe { self.force_get() };
            }
        }

        self
            .poll()
            .unwrap_or_else(|| unreachable!("Encountered INCOMPLETE when polling Once"))
    }

    /// Returns a reference to the inner value if the [`Once`] has been initialized.
    pub fn get(&self) -> Option<&T> {
        match self.state.load(Ordering::SeqCst) {
            COMPLETE => Some(unsafe { self.force_get() }),
            _ => None,
        }
    }

    /// Returns a mutable reference to the inner value if the [`Once`] has been initialized.
    ///
    /// Because this method requires a mutable reference to the [`Once`], no synchronization
    /// overhead is required to access the inner value. In effect, it is zero-cost.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        match *self.state.get_mut() {
            COMPLETE => Some(unsafe { self.force_get_mut() }),
            _ => None,
        }
    }

    /// Returns a the inner value if the [`Once`] has been initialized.
    ///
    /// Because this method requires ownershup of the [`Once`], no synchronization overhead
    /// is required to access the inner value. In effect, it is zero-cost.
    pub fn try_into_inner(mut self) -> Option<T> {
        match *self.state.get_mut() {
            COMPLETE => Some(unsafe { self.force_into_inner() }),
            _ => None,
        }
    }

    /// Returns a reference to the inner value if the [`Once`] has been initialized.
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::SeqCst) == COMPLETE
    }

    /// Spins until the [`Once`] contains a value.
    ///
    /// Note that in releases prior to `0.7`, this function had the behaviour of [`Once::poll`].
    ///
    /// # Panics
    ///
    /// This function will panic if the [`Once`] previously panicked while attempting
    /// to initialize. This is similar to the poisoning behaviour of `std::sync`'s
    /// primitives.
    pub fn wait(&self) -> &T {
        loop {
            match self.poll() {
                Some(x) => break x,
                None => crate::relax(),
            }
        }
    }

    /// Like [`Once::get`], but will spin if the [`Once`] is in the process of being
    /// initialized. If initialization has not even begun, `None` will be returned.
    ///
    /// Note that in releases prior to `0.7`, this function was named `wait`.
    ///
    /// # Panics
    ///
    /// This function will panic if the [`Once`] previously panicked while attempting
    /// to initialize. This is similar to the poisoning behaviour of `std::sync`'s
    /// primitives.
    pub fn poll(&self) -> Option<&T> {
        loop {
            match self.state.load(Ordering::SeqCst) {
                INCOMPLETE => return None,
                RUNNING => crate::relax(), // We spin
                COMPLETE => return Some(unsafe { self.force_get() }),
                PANICKED => panic!("Once previously poisoned by a panicked"),
                _ => unsafe { unreachable() },
            }
        }
    }
}

impl<T> From<T> for Once<T> {
    fn from(data: T) -> Self {
        Self::initialized(data)
    }
}

impl<T> Drop for Once<T> {
    fn drop(&mut self) {
        if self.state.load(Ordering::SeqCst) == COMPLETE {
            unsafe {
                //TODO: Use MaybeUninit::assume_init_drop once stabilised
                core::ptr::drop_in_place((*self.data.get()).as_mut_ptr());
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
    fn get() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.get().is_none());
        INIT.call_once(|| 2);
        assert_eq!(INIT.get().map(|r| *r), Some(2));
    }

    #[test]
    fn get_no_wait() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.get().is_none());
        thread::spawn(move|| {
            INIT.call_once(|| loop { });
        });
        assert!(INIT.get().is_none());
    }


    #[test]
    fn poll() {
        static INIT: Once<usize> = Once::new();

        assert!(INIT.poll().is_none());
        INIT.call_once(|| 3);
        assert_eq!(INIT.poll().map(|r| *r), Some(3));
    }


    #[test]
    fn wait() {
        static INIT: Once<usize> = Once::new();

        std::thread::spawn(|| {
            assert_eq!(*INIT.wait(), 3);
            assert!(INIT.is_completed());
        });

        for _ in 0..4 { thread::yield_now() }

        assert!(INIT.poll().is_none());
        INIT.call_once(|| 3);
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

    #[test]
    fn init_constant() {
        static O: Once<()> = Once::INIT;
        let mut a = 0;
        O.call_once(|| a += 1);
        assert_eq!(a, 1);
        O.call_once(|| a += 1);
        assert_eq!(a, 1);
    }

    static mut CALLED: bool = false;

    struct DropTest {}

    impl Drop for DropTest {
        fn drop(&mut self) {
            unsafe {
                CALLED = true;
            }
        }
    }

    #[test]
    fn drop() {
        unsafe {
            CALLED = false;
        }

        {
            let once = Once::new();
            once.call_once(|| DropTest {});
        }

        assert!(unsafe {
            CALLED
        });
    }

    #[test]
    fn skip_uninit_drop() {
        unsafe {
            CALLED = false;
        }

        {
            let once = Once::<DropTest>::new();
        }

        assert!(unsafe {
            !CALLED
        });
    }
}
