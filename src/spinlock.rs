#![crate_type="lib"]
#![feature(phase)]

use std::sync::atomics::{AtomicBool, SeqCst};
use std::ty::Unsafe;
use std::rt::local::Local;
use std::rt::task::Task;

pub struct SpinLock<T>
{
    lock: AtomicBool,
    data: Unsafe<T>,
}

impl<T: Send> SpinLock<T>
{
    pub fn new(user_data: T) -> SpinLock<T>
    {
        SpinLock
        {
            lock: AtomicBool::new(false),
            data: Unsafe::new(user_data),
        }
    }

    pub fn access(&self, closure: |&mut T|)
    {
        while self.lock.swap(true, SeqCst) == true
        {
            if Local::exists(None::<Task>)
            {
                std::task::deschedule();
            }
        }

        let data_ref = unsafe { &mut *self.data.get() };
        closure(data_ref);

        self.lock.store(false, SeqCst);
    }
}
