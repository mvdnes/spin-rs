use core::sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::fmt;
use core::default::Default;

use util::cpu_relax;

/// A reader-writer lock
///
/// This type of lock allows a number of readers or at most one writer at any
/// point in time. The write portion of this lock typically allows modification
/// of the underlying data (exclusive access) and the read portion of this lock
/// typically allows for read-only access (shared access).
///
/// The type parameter `T` represents the data that this lock protects. It is
/// required that `T` satisfies `Send` to be shared across tasks and `Sync` to
/// allow concurrent access through readers. The RAII guards returned from the
/// locking methods implement `Deref` (and `DerefMut` for the `write` methods)
/// to allow access to the contained of the lock.
///
/// Based on
/// https://jfdube.wordpress.com/2014/01/03/implementing-a-recursive-read-write-spinlock/
///
/// # Examples
///
/// ```
/// use spin;
///
/// let lock = spin::RwLock::new(5);
///
/// // many reader locks can be held at once
/// {
///     let r1 = lock.read();
///     let r2 = lock.read();
///     assert_eq!(*r1, 5);
///     assert_eq!(*r2, 5);
/// } // read locks are dropped at this point
///
/// // only one write lock may be held, however
/// {
///     let mut w = lock.write();
///     *w += 1;
///     assert_eq!(*w, 6);
/// } // write lock is dropped here
/// ```
pub struct RwLock<T: ?Sized>
{
    lock: AtomicUsize,
    data: UnsafeCell<T>,
}

/// A guard to which the protected data can be read
///
/// When the guard falls out of scope it will decrement the read count,
/// potentially releasing the lock.
pub struct RwLockReadGuard<'a, T: 'a + ?Sized>
{
    lock: &'a AtomicUsize,
    data: &'a T,
}

/// A guard to which the protected data can be written
///
/// When the guard falls out of scope it will release the lock.
pub struct RwLockWriteGuard<'a, T: 'a + ?Sized>
{
    lock: &'a AtomicUsize,
    data: &'a mut T,
}

// Same unsafe impls as `std::sync::RwLock`
unsafe impl<T: ?Sized + Send + Sync> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

const USIZE_MSB: usize = ::core::isize::MIN as usize;

impl<T> RwLock<T>
{
    /// Creates a new spinlock wrapping the supplied data.
    ///
    /// May be used statically:
    ///
    /// ```
    /// #![feature(const_fn)]
    /// use spin;
    ///
    /// static RW_LOCK: spin::RwLock<()> = spin::RwLock::new(());
    ///
    /// fn demo() {
    ///     let lock = RW_LOCK.read();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline]
    #[cfg(feature = "const_fn")]
    pub const fn new(user_data: T) -> RwLock<T>
    {
        RwLock
        {
            lock: ATOMIC_USIZE_INIT,
            data: UnsafeCell::new(user_data),
        }
    }

    /// Creates a new spinlock wrapping the supplied data.
    ///
    /// If you want to use it statically, you can use the `const_fn` feature.
    ///
    /// ```
    /// use spin;
    ///
    /// fn demo() {
    ///     let rw_lock = spin::RwLock::new(());
    ///     let lock = rw_lock.read();
    ///     // do something with lock
    ///     drop(lock);
    /// }
    /// ```
    #[inline]
    #[cfg(not(feature = "const_fn"))]
    pub fn new(user_data: T) -> RwLock<T>
    {
        RwLock
        {
            lock: ATOMIC_USIZE_INIT,
            data: UnsafeCell::new(user_data),
        }
    }

    /// Consumes this `RwLock`, returning the underlying data.
    pub fn into_inner(self) -> T
    {
        // We know statically that there are no outstanding references to
        // `self` so there's no need to lock.
        let RwLock { data, .. } = self;
        unsafe { data.into_inner() }
    }
}

impl<T: ?Sized> RwLock<T>
{
    /// Locks this rwlock with shared read access, blocking the current thread
    /// until it can be acquired.
    ///
    /// The calling thread will be blocked until there are no more writers which
    /// hold the lock. There may be other readers currently inside the lock when
    /// this method returns. This method does not provide any guarantees with
    /// respect to the ordering of whether contentious readers or writers will
    /// acquire the lock first.
    ///
    /// Returns an RAII guard which will release this thread's shared access
    /// once it is dropped.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     let mut data = mylock.read();
    ///     // The lock is now locked and the data can be read
    ///     println!("{}", *data);
    ///     // The lock is dropped
    /// }
    /// ```
    #[inline]
    pub fn read<'a>(&'a self) -> RwLockReadGuard<'a, T>
    {
        // (funny do-while loop)
        while {
            // Old value, with write bit unset
            let mut old;

            // Wait for for writer to go away before doing expensive atomic ops
            // (funny do-while loop)
            while {
                old = self.lock.load(Ordering::Relaxed);
                old & USIZE_MSB != 0
            } {
                cpu_relax();
            }

            // unset write bit
            old &= !USIZE_MSB;

            let new = old + 1;
            debug_assert!(new != (!USIZE_MSB) & (!0));

            self.lock.compare_and_swap(old, new, Ordering::SeqCst) != old
        } {
            cpu_relax();
        }
        RwLockReadGuard {
            lock: &self.lock,
            data: unsafe { & *self.data.get() },
        }
    }

    /// Attempt to acquire this lock with shared read access.
    ///
    /// This function will never block and will return immediately if `read`
    /// would otherwise succeed. Returns `Some` of an RAII guard which will
    /// release the shared access of this thread when dropped, or `None` if the
    /// access could not be granted. This method does not provide any
    /// guarantees with respect to the ordering of whether contentious readers
    /// or writers will acquire the lock first.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     match mylock.try_read() {
    ///         Some(data) => {
    ///             // The lock is now locked and the data can be read
    ///             println!("{}", *data);
    ///             // The lock is dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub fn try_read(&self) -> Option<RwLockReadGuard<T>>
    {
        // Old value, with write bit unset
        let old = (!USIZE_MSB) & self.lock.load(Ordering::Relaxed);

        let new = old + 1;
        debug_assert!(new != (!USIZE_MSB) & (!0));
        if self.lock.compare_and_swap(old,
                                      new,
                                      Ordering::SeqCst) == old
        {
            Some(RwLockReadGuard {
                lock: &self.lock,
                data: unsafe { & *self.data.get() },
            })
        } else {
            None
        }
    }

    /// Force decrement the reader count.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockReadGuard`s
    /// live, or if called more times than `read` has been called, but can be
    /// useful in FFI contexts where the caller doesn't know how to deal with
    /// RAII.
    pub unsafe fn force_read_decrement(&self) {
        debug_assert!(self.lock.load(Ordering::Relaxed) & (!USIZE_MSB) > 0);
        self.lock.fetch_sub(1, Ordering::SeqCst);
    }

    /// Force unlock exclusive write access.
    ///
    /// This is *extremely* unsafe if there are outstanding `RwLockWriteGuard`s
    /// live, or if called when there are current readers, but can be useful in
    /// FFI contexts where the caller doesn't know how to deal with RAII.
    pub unsafe fn force_write_unlock(&self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed), USIZE_MSB);
        self.lock.store(0, Ordering::Relaxed);
    }

    /// Lock this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     let mut data = mylock.write();
    ///     // The lock is now locked and the data can be written
    ///     *data += 1;
    ///     // The lock is dropped
    /// }
    /// ```
    #[inline]
    pub fn write<'a>(&'a self) -> RwLockWriteGuard<'a, T>
    {
        loop
        {
            // Old value, with write bit unset.
            let old = (!USIZE_MSB) & self.lock.load(Ordering::Relaxed);
            // Old value, with write bit set.
            let new = USIZE_MSB | old;
            if self.lock.compare_and_swap(old,
                                          new,
                                          Ordering::SeqCst) == old
            {
                // Wait for readers to go away, then lock is ours.
                while self.lock.load(Ordering::Relaxed) != USIZE_MSB {
                    cpu_relax();
                }
                break
            }
        }
        RwLockWriteGuard {
            lock: &self.lock,
            data: unsafe { &mut *self.data.get() },
        }
    }

    /// Attempt to lock this rwlock with exclusive write access.
    ///
    /// This function does not ever block, and it will return `None` if a call
    /// to `write` would otherwise block. If successful, an RAII guard is
    /// returned.
    ///
    /// ```
    /// let mylock = spin::RwLock::new(0);
    /// {
    ///     match mylock.try_write() {
    ///         Some(mut data) => {
    ///             // The lock is now locked and the data can be written
    ///             *data += 1;
    ///             // The lock is implicitly dropped
    ///         },
    ///         None => (), // no cigar
    ///     };
    /// }
    /// ```
    #[inline]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>>
    {
        if self.lock.compare_and_swap(0,
                                      USIZE_MSB,
                                      Ordering::SeqCst) == 0
        {
            Some(RwLockWriteGuard {
                lock: &self.lock,
                data: unsafe { &mut *self.data.get() },
            })
        } else {
            None
        }
    }
}

impl<T: ?Sized + fmt::Debug> fmt::Debug for RwLock<T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self.try_read()
        {
            Some(guard) => write!(f, "RwLock {{ data: {:?} }}", &*guard),
            None => write!(f, "RwLock {{ <locked> }}"),
        }
    }
}

impl<T: ?Sized + Default> Default for RwLock<T> {
    fn default() -> RwLock<T> {
        RwLock::new(Default::default())
    }
}

impl<'rwlock, T: ?Sized> Deref for RwLockReadGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data }
}

impl<'rwlock, T: ?Sized> Deref for RwLockWriteGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data }
}

impl<'rwlock, T: ?Sized> DerefMut for RwLockWriteGuard<'rwlock, T> {
    fn deref_mut(&mut self) -> &mut T { self.data }
}

impl<'rwlock, T: ?Sized> Drop for RwLockReadGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert!(self.lock.load(Ordering::Relaxed) & (!USIZE_MSB) > 0);
        self.lock.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<'rwlock, T: ?Sized> Drop for RwLockWriteGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed), USIZE_MSB);
        self.lock.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;

    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use super::*;

    #[derive(Eq, PartialEq, Debug)]
    struct NonCopy(i32);

    #[test]
    fn smoke() {
        let l = RwLock::new(());
        drop(l.read());
        drop(l.write());
        drop((l.read(), l.read()));
        drop(l.write());
    }

    // TODO: needs RNG
    //#[test]
    //fn frob() {
    //    static R: RwLock = RwLock::new();
    //    const N: usize = 10;
    //    const M: usize = 1000;
    //
    //    let (tx, rx) = channel::<()>();
    //    for _ in 0..N {
    //        let tx = tx.clone();
    //        thread::spawn(move|| {
    //            let mut rng = rand::thread_rng();
    //            for _ in 0..M {
    //                if rng.gen_weighted_bool(N) {
    //                    drop(R.write());
    //                } else {
    //                    drop(R.read());
    //                }
    //            }
    //            drop(tx);
    //        });
    //    }
    //    drop(tx);
    //    let _ = rx.recv();
    //    unsafe { R.destroy(); }
    //}

    #[test]
    fn test_rw_arc() {
        let arc = Arc::new(RwLock::new(0));
        let arc2 = arc.clone();
        let (tx, rx) = channel();

        thread::spawn(move|| {
            let mut lock = arc2.write();
            for _ in 0..10 {
                let tmp = *lock;
                *lock = -1;
                thread::yield_now();
                *lock = tmp + 1;
            }
            tx.send(()).unwrap();
        });

        // Readers try to catch the writer in the act
        let mut children = Vec::new();
        for _ in 0..5 {
            let arc3 = arc.clone();
            children.push(thread::spawn(move|| {
                let lock = arc3.read();
                assert!(*lock >= 0);
            }));
        }

        // Wait for children to pass their asserts
        for r in children {
            assert!(r.join().is_ok());
        }

        // Wait for writer to finish
        rx.recv().unwrap();
        let lock = arc.read();
        assert_eq!(*lock, 10);
    }

    #[test]
    fn test_rw_arc_access_in_unwind() {
        let arc = Arc::new(RwLock::new(1));
        let arc2 = arc.clone();
        let _ = thread::spawn(move|| -> () {
            struct Unwinder {
                i: Arc<RwLock<isize>>,
            }
            impl Drop for Unwinder {
                fn drop(&mut self) {
                    let mut lock = self.i.write();
                    *lock += 1;
                }
            }
            let _u = Unwinder { i: arc2 };
            panic!();
        }).join();
        let lock = arc.read();
        assert_eq!(*lock, 2);
    }

    #[test]
    fn test_rwlock_unsized() {
        let rw: &RwLock<[i32]> = &RwLock::new([1, 2, 3]);
        {
            let b = &mut *rw.write();
            b[0] = 4;
            b[2] = 5;
        }
        let comp: &[i32] = &[4, 2, 5];
        assert_eq!(&*rw.read(), comp);
    }

    #[test]
    fn test_rwlock_try_write() {
        use std::mem::drop;

        let lock = RwLock::new(0isize);
        let read_guard = lock.read();

        let write_result = lock.try_write();
        match write_result {
            None => (),
            Some(_) => assert!(false, "try_write should not succeed while read_guard is in scope"),
        }

        drop(read_guard);
    }

    #[test]
    fn test_into_inner() {
        let m = RwLock::new(NonCopy(10));
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
        let m = RwLock::new(Foo(num_drops.clone()));
        assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        {
            let _inner = m.into_inner();
            assert_eq!(num_drops.load(Ordering::SeqCst), 0);
        }
        assert_eq!(num_drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_force_read_decrement() {
        let m = RwLock::new(());
        ::std::mem::forget(m.read());
        ::std::mem::forget(m.read());
        ::std::mem::forget(m.read());
        assert!(m.try_write().is_none());
        unsafe {
            m.force_read_decrement();
            m.force_read_decrement();
        }
        assert!(m.try_write().is_none());
        unsafe {
            m.force_read_decrement();
        }
        assert!(m.try_write().is_some());
    }

    #[test]
    fn test_force_write_unlock() {
        let m = RwLock::new(());
        ::std::mem::forget(m.write());
        assert!(m.try_read().is_none());
        unsafe {
            m.force_write_unlock();
        }
        assert!(m.try_read().is_some());
    }
}
