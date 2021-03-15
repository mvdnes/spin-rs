//! Strategies that determine the behaviour of locks when encountering contention.

/// A trait implemented by spinning relax strategies.
pub trait RelaxStrategy {
    /// Perform the relaxing operation during a period of contention.
    fn relax();
}

/// A strategy that involves rapidly spinning while informing the CPU that it should power down non-essential
/// components via [`spin_loop_hint`]. Note that spinning is a 'dumb' strategy and most schedulers cannot connectly
/// differentiate it from useful work. If you see signs that priority inversion is occurring, consider switching to
/// [`Yield`] or, even better, not using a spinlock at all and opting for a proper mutex.
pub struct Spin;

impl RelaxStrategy for Spin {
    #[inline(always)]
    fn relax() {
        core::hint::spin_loop();
    }
}

/// A strategy that yields the current time slice to the scheduler in favour of other threads or processes. This is
/// generally used as a strategy for minimising power consumption and priority inversion on targets that have a
/// standard library available. Note that such targets have scheduler-integrated concurrency primitives available, and
/// you should generally use these instead, except in rare circumstances.
#[cfg(feature = "std")]
pub struct Yield;

#[cfg(feature = "std")]
impl RelaxStrategy for Yield {
    #[inline(always)]
    fn relax() {
        std::thread::yield_now();
    }
}
