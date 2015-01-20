use core::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT};
use core::cell::UnsafeCell;
use core::marker::Sync;
use core::ops::{Drop, Deref, DerefMut};

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
pub struct RWLockReadGuard<'a, T:'a>
{
    lock: &'a AtomicUsize,
    data: &'a T,
}

/// A guard to which the protected data can be written
///
/// When the guard falls out of scope it will release the lock.
pub struct RWLockWriteGuard<'a, T:'a>
{
    lock: &'a AtomicUsize,
    data: &'a mut T,
}

#[allow(unstable)]
unsafe impl<T> Sync for RwLock<T> {}

/// A RwLock which may be used statically.
///
/// ```
/// use spin::{StaticRwLock, STATIC_MUTEX_INIT};
///
/// static SPLCK: StaticRwLock = STATIC_MUTEX_INIT;
///
/// fn demo() {
///     let lock = SPLCK.lock();
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
    pub fn read<'a>(&'a self) -> RWLockReadGuard<'a, T>
    {
        loop
        {
            // old value, with write bit unset
            let old = (!USIZE_MSB) & self.lock.load(Ordering::Relaxed);
            if self.lock.compare_and_swap(old, old + 1, Ordering::SeqCst) == old {
                break;
            }
        }
        RWLockReadGuard {
            lock: &self.lock,
            data: unsafe { & *self.data.get() },
        }
    }
}
