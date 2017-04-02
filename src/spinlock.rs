use core::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};
use core::marker::Sync;
use core::default::Default;
use lock::Lock;
use util::cpu_relax;

pub struct SpinLock
{
    lock: AtomicBool,
}

unsafe impl Sync for SpinLock {}
unsafe impl Send for SpinLock {}

impl SpinLock
{
    #[cfg(feature = "const_fn")]
    pub const fn new() -> SpinLock
    {
        SpinLock { lock: ATOMIC_BOOL_INIT }
    }

    #[cfg(not(feature = "const_fn"))]
    pub fn new() -> SpinLock
    {
        SpinLock { lock: ATOMIC_BOOL_INIT }
    }
}

impl Lock for SpinLock
{
    #[inline]
    fn try_lock(&self) -> bool
    {
        self.lock
            .compare_and_swap(false, true, Ordering::Acquire) == false
    }

    #[inline]
    fn lock(&self)
    {
        while !self.try_lock()
        {
            cpu_relax();
        }
    }

    #[inline]
    fn unlock(&self)
    {
        self.lock.store(false, Ordering::Release);
    }
}

impl Default for SpinLock
{
    fn default() -> SpinLock
    {
        SpinLock::new()
    }
}
