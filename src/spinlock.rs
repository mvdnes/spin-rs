#![crate_type = "lib"]
#![feature(unsafe_destructor)]

use std::sync::atomics::{AtomicBool, SeqCst};
use std::ty::Unsafe;
use std::rt::local::Local;
use std::rt::task::Task;

pub struct Spinlock<T>
{
    lock: AtomicBool,
    data: Unsafe<T>,
}

pub struct SpinlockGuard<'a, T>
{
    lock: &'a AtomicBool,
    data: &'a mut T,
}

impl<T: Send> Spinlock<T>
{
    pub fn new(user_data: T) -> Spinlock<T>
    {
        Spinlock
        {
            lock: AtomicBool::new(false),
            data: Unsafe::new(user_data),
        }
    }

    fn _lock(&self)
    {
        while self.lock.swap(true, SeqCst) == true
        {
            if Local::exists(None::<Task>)
            {
                std::task::deschedule();
            }
        }
    }

    pub fn lock<'a>(&'a self) -> SpinlockGuard<'a, T>
    {
        self._lock();
        SpinlockGuard
        {
            lock: &self.lock,
            data: unsafe { &mut *self.data.get() },
        }
    }
}

impl<'a, T: Send> Deref<T> for SpinlockGuard<'a, T>
{
    fn deref<'a>(&'a self) -> &'a T { &*self.data }
}

impl<'a, T: Send> DerefMut<T> for SpinlockGuard<'a, T>
{
    fn deref_mut<'a>(&'a mut self) -> &'a mut T { &mut *self.data }
}

#[unsafe_destructor]
impl<'a, T: Send> Drop for SpinlockGuard<'a, T>
{
    fn drop(&mut self)
    {
        self.lock.store(false, SeqCst);
    }
}
