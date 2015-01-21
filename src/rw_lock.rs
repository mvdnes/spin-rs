#![allow(unstable)]

use core::prelude::*;

use core::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};

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
pub struct RwLock<T>
{
    lock: AtomicUsize,
    data: UnsafeCell<T>,
}

/// A guard to which the protected data can be read
///
/// When the guard falls out of scope it will decrement the read count,
/// potentially releasing the lock.
///
/// Based on
/// https://jfdube.wordpress.com/2014/01/03/implementing-a-recursive-read-write-spinlock/
///
pub struct RwLockReadGuard<'a, T:'a>
{
    lock: &'a AtomicUsize,
    data: &'a T,
}

/// A guard to which the protected data can be written
///
/// When the guard falls out of scope it will release the lock.
pub struct RwLockWriteGuard<'a, T:'a>
{
    lock: &'a AtomicUsize,
    data: &'a mut T,
}

unsafe impl<T> Sync for RwLock<T> {}
unsafe impl<T:'static+Send> Send for RwLock<T> {}

/// A RwLock which may be used statically.
///
/// ```
/// use spin::{StaticRwLock, STATIC_RWLOCK_INIT};
///
/// static SPRWLCK: StaticRwLock = STATIC_RWLOCK_INIT;
///
/// fn demo() {
///     let lock = SPRWLCK.read();
///     // do something with lock
///     drop(lock);
/// }
/// ```
pub type StaticRwLock = RwLock<()>;

/// A initializer for StaticRwLock, containing no data.
pub const STATIC_RWLOCK_INIT: StaticRwLock = RwLock {
    lock: ATOMIC_USIZE_INIT,
    data: UnsafeCell { value: () },
};

const USIZE_MSB: usize = ::core::isize::MIN as usize;

impl<T> RwLock<T>
{
    /// Creates a new spinlock wrapping the supplied data.
    #[inline]
    pub fn new(user_data: T) -> RwLock<T>
    {
        RwLock
        {
            lock: ATOMIC_USIZE_INIT,
            data: UnsafeCell::new(user_data),
        }
    }

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
    #[inline]
    pub fn read<'a>(&'a self) -> RwLockReadGuard<'a, T>
    {
        loop
        {
            match self.try_read() {
                None        => (),
                Some(guard) => return guard,
            }
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
    #[inline]
    pub fn try_read(&self) -> Option<RwLockReadGuard<T>> {
        // Old value, with write bit unset
        let old = (!USIZE_MSB) & self.lock.load(Ordering::Relaxed);
        let new = old + 1;
        debug_assert!(new != (!USIZE_MSB) & (-1));
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

    /// Lock this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    ///
    /// This function will not return while other writers or other readers
    /// currently have access to the lock.
    ///
    /// Returns an RAII guard which will drop the write access of this rwlock
    /// when dropped.
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
                while self.lock.load(Ordering::Relaxed) != USIZE_MSB { }
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
    #[inline]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<T>> {
        // No readers or writers.
        let old = 0;
        // Old value, with write bit set.
        let new = USIZE_MSB | old;
        if self.lock.compare_and_swap(old,
                                      new,
                                      Ordering::SeqCst) == old
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

impl<'rwlock, T> Deref for RwLockReadGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data }
}

impl<'rwlock, T> Deref for RwLockWriteGuard<'rwlock, T> {
    type Target = T;

    fn deref(&self) -> &T { self.data }
}

impl<'rwlock, T> DerefMut for RwLockWriteGuard<'rwlock, T> {
    fn deref_mut(&mut self) -> &mut T { self.data }
}

#[unsafe_destructor]
impl<'rwlock, T> Drop for RwLockReadGuard<'rwlock, T> {
    fn drop(&mut self) {
        self.lock.fetch_sub(1, Ordering::SeqCst);
    }
}

#[unsafe_destructor]
impl<'rwlock, T> Drop for RwLockWriteGuard<'rwlock, T> {
    fn drop(&mut self) {
        debug_assert_eq!(self.lock.load(Ordering::Relaxed), USIZE_MSB);
        self.lock.store(0, Ordering::Relaxed);
    }
}
