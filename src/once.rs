    //! Synchronization primitives for one-time evaluation.

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
    marker::PhantomData,
    fmt,
};
use crate::{RelaxStrategy, Spin};

/// A primitive that provides lazy one-time initialization.
///
/// Unlike its `std::sync` equivalent, this is generalized such that the closure returns a
/// value to be stored by the [`Once`] (`std::sync::Once` can be trivially emulated with
/// `Once`).
///
/// Because [`Once::new`] is `const`, this primitive may be used to safely initialize statics.
///
/// # Examples
///
/// ```
/// use spin;
///
/// static START: spin::Once = spin::Once::new();
///
/// START.call_once(|| {
///     // run initialization here
/// });
/// ```
pub struct Once<T = (), R = Spin> {
    phantom: PhantomData<R>,
    status: AtomicUsize,
    data: UnsafeCell<MaybeUninit<T>>,
}

impl<T: fmt::Debug, R> fmt::Debug for Once<T, R> {
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
unsafe impl<T: Send + Sync, R> Sync for Once<T, R> {}
unsafe impl<T: Send, R> Send for Once<T, R> {}

// Four states that a Once can be in, encoded into the lower bits of `status` in
// the Once structure.
const INCOMPLETE: usize = 0x0;
const RUNNING: usize = 0x1;
const COMPLETE: usize = 0x2;
const PANICKED: usize = 0x3;

use core::hint::unreachable_unchecked as unreachable;

impl<T, R: RelaxStrategy> Once<T, R> {
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
        // SAFETY: We perform an Acquire load because if this were to return COMPLETE, then we need
        // the preceding stores done while initializing, to become visible after this load.
        let mut status = self.status.load(Ordering::Acquire);

        if status == INCOMPLETE {
            match self.status.compare_exchange(
                INCOMPLETE,
                RUNNING,
                // SAFETY: Success ordering: We do not have to synchronize any data at all, as the
                // value is at this point uninitialized, so Relaxed is technically sufficient. We
                // will however have to do a Release store later. However, the success ordering
                // must always be at least as strong as the failure ordering, so we choose Acquire
                // here anyway.
                Ordering::Acquire,
                // SAFETY: Failure ordering: While we have already loaded the status initially, we
                // know that if some other thread would have fully initialized this in between,
                // then there will be new not-yet-synchronized accesses done during that
                // initialization that would not have been synchronized by the earlier load. Thus
                // we use Acquire to ensure when we later call force_get() in the last match
                // statement, if the status was changed to COMPLETE, that those accesses will become
                // visible to us.
                Ordering::Acquire,
            ) {
                Ok(_must_be_state_incomplete) => {
                    // The compare-exchange suceeded, so we shall initialize it.

                    // We use a guard (Finish) to catch panics caused by builder
                    let mut finish = Finish { status: &self.status, panicked: true };
                    unsafe {
                        // SAFETY:
                        // `UnsafeCell`/deref: currently the only accessor, mutably
                        // and immutably by cas exclusion.
                        // `write`: pointer comes from `MaybeUninit`.
                        (*self.data.get()).as_mut_ptr().write(f())
                    };
                    finish.panicked = false;

                    // SAFETY: Release is required here, so that all memory accesses done in the
                    // closure when initializing, become visible to other threads that perform Acquire
                    // loads.
                    //
                    // And, we also know that the changes this thread has done will not magically
                    // disappear from our cache, so it does not need to be AcqRel.
                    self.status.store(COMPLETE, Ordering::Release);

                    // This next line is mainly an optimization.
                    return unsafe { self.force_get() };
                }
                // The compare-exchange failed, so we know for a fact that the status cannot be
                // INCOMPLETE, or it would have succeeded.
                Err(other_status) => status = other_status,
            }
        }

        match status {
            // SAFETY: We have either checked with an Acquire load, that the status is COMPLETE, or
            // initialized it ourselves, in which case no additional synchronization is needed.
            COMPLETE => unsafe { self.force_get() },
            PANICKED => panic!("Once panicked"),
            RUNNING => self
                .poll()
                .unwrap_or_else(|| {
                    if cfg!(debug_assertions) {
                        unreachable!("Encountered INCOMPLETE when polling Once")
                    } else {
                        // SAFETY: This poll is guaranteed never to fail because the API of poll
                        // promises spinning if initialization is in progress. We've already
                        // checked that initialisation is in progress, and initialisation is
                        // monotonic: once done, it cannot be undone. We also fetched the status
                        // with Acquire semantics, thereby guaranteeing that the later-executed
                        // poll will also agree with us that initialization is in progress. Ergo,
                        // this poll cannot fail.
                        unsafe {
                            unreachable();
                        }
                    }
                }),

            // SAFETY: The only invariant possible in addition to the aforementioned ones at the
            // moment, is INCOMPLETE. However, the only way for this match statement to be
            // reached, is if we lost the CAS (otherwise we would have returned early), in
            // which case we know for a fact that the state cannot be changed back to INCOMPLETE as
            // `Once`s are monotonic.
            _ => unsafe { unreachable() },
        }

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
                None => R::relax(),
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
            // SAFETY: Acquire is safe here, because if the status is COMPLETE, then we want to make
            // sure that all memory accessed done while initializing that value, are visible when
            // we return a reference to the inner data after this load.
            match self.status.load(Ordering::Acquire) {
                INCOMPLETE => return None,
                RUNNING => R::relax(), // We spin
                COMPLETE => return Some(unsafe { self.force_get() }),
                PANICKED => panic!("Once previously poisoned by a panicked"),
                _ => unsafe { unreachable() },
            }
        }
    }
}

impl<T, R> Once<T, R> {
    /// Initialization constant of [`Once`].
    #[allow(clippy::declare_interior_mutable_const)]
    pub const INIT: Self = Self {
        phantom: PhantomData,
        status: AtomicUsize::new(INCOMPLETE),
        data: UnsafeCell::new(MaybeUninit::uninit()),
    };

    /// Creates a new [`Once`].
    pub const fn new() -> Self{
        Self::INIT
    }

    /// Creates a new initialized [`Once`].
    pub const fn initialized(data: T) -> Self {
        Self {
            phantom: PhantomData,
            status: AtomicUsize::new(COMPLETE),
            data: UnsafeCell::new(MaybeUninit::new(data)),
        }
    }

    /// Retrieve a pointer to the inner data.
    ///
    /// While this method itself is safe, accessing the pointer before the [`Once`] has been
    /// initialized is UB, unless this method has already been written to from a pointer coming
    /// from this method.
    pub fn as_mut_ptr(&self) -> *mut T {
        // SAFETY:
        // * MaybeUninit<T> always has exactly the same layout as T
        self.data.get().cast::<T>()
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

    /// Returns a reference to the inner value if the [`Once`] has been initialized.
    pub fn get(&self) -> Option<&T> {
        // SAFETY: Just as with `poll`, Acquire is safe here because we want to be able to see the
        // nonatomic stores done when initializing, once we have loaded and checked the status.
        match self.status.load(Ordering::Acquire) {
            COMPLETE => Some(unsafe { self.force_get() }),
            _ => None,
        }
    }

    /// Returns a reference to the inner value on the unchecked assumption that the  [`Once`] has been initialized.
    ///
    /// # Safety
    ///
    /// This is *extremely* unsafe if the `Once` has not already been initialized because a reference to uninitialized
    /// memory will be returned, immediately triggering undefined behaviour (even if the reference goes unused).
    /// However, this can be useful in some instances for exposing the `Once` to FFI or when the overhead of atomically
    /// checking initialization is unacceptable and the `Once` has already been initialized.
    pub unsafe fn get_unchecked(&self) -> &T {
        debug_assert_eq!(
            self.status.load(Ordering::SeqCst),
            COMPLETE,
            "Attempted to access an uninitialized Once. If this was run without debug checks, this would be undefined behaviour. This is a serious bug and you must fix it.",
        );
        self.force_get()
    }

    /// Returns a mutable reference to the inner value if the [`Once`] has been initialized.
    ///
    /// Because this method requires a mutable reference to the [`Once`], no synchronization
    /// overhead is required to access the inner value. In effect, it is zero-cost.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        match *self.status.get_mut() {
            COMPLETE => Some(unsafe { self.force_get_mut() }),
            _ => None,
        }
    }

    /// Returns a the inner value if the [`Once`] has been initialized.
    ///
    /// Because this method requires ownership of the [`Once`], no synchronization overhead
    /// is required to access the inner value. In effect, it is zero-cost.
    pub fn try_into_inner(mut self) -> Option<T> {
        match *self.status.get_mut() {
            COMPLETE => Some(unsafe { self.force_into_inner() }),
            _ => None,
        }
    }

    /// Checks whether the value has been initialized.
    ///
    /// This is done using [`Acquire`](core::sync::atomic::Ordering::Acquire) ordering, and
    /// therefore it is safe to access the value directly via
    /// [`get_unchecked`](Self::get_unchecked) if this returns true.
    pub fn is_completed(&self) -> bool {
        // TODO: Add a similar variant for Relaxed?
        self.status.load(Ordering::Acquire) == COMPLETE
    }
}

impl<T, R> From<T> for Once<T, R> {
    fn from(data: T) -> Self {
        Self::initialized(data)
    }
}

impl<T, R> Drop for Once<T, R> {
    fn drop(&mut self) {
        // No need to do any atomic access here, we have &mut!
        if *self.status.get_mut() == COMPLETE {
            unsafe {
                //TODO: Use MaybeUninit::assume_init_drop once stabilised
                core::ptr::drop_in_place((*self.data.get()).as_mut_ptr());
            }
        }
    }
}

struct Finish<'a> {
    status: &'a AtomicUsize,
    panicked: bool,
}

impl<'a> Drop for Finish<'a> {
    fn drop(&mut self) {
        if self.panicked {
            // While using Relaxed here would most likely not be an issue, we use SeqCst anyway.
            // This is mainly because panics are not meant to be fast at all, but also because if
            // there were to be a compiler bug which reorders accesses within the same thread,
            // where it should not, we want to be sure that the panic really is handled, and does
            // not cause additional problems. SeqCst will therefore help guarding against such
            // bugs.
            self.status.store(PANICKED, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    use std::sync::mpsc::channel;
    use std::thread;

    use super::*;

    #[test]
    fn smoke_once() {
        static O: Once = Once::new();
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
        static O: Once = Once::new();
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

        static INIT: Once = Once::new();

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
        static O: Once = Once::INIT;
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
    fn drop_occurs() {
        unsafe {
            CALLED = false;
        }

        {
            let once = Once::<_>::new();
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

        let once = Once::<DropTest>::new();
        drop(once);

        assert!(unsafe {
            !CALLED
        });
    }
}
