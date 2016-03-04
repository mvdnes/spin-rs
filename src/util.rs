/// Called while spinning (name borrowed from Linux). Can be implemented to call
/// a platform-specific method of lightening CPU load in spinlocks.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline(always)]
pub fn cpu_relax() {
    // This instruction is meant for usage in spinlock loops
    // (see Intel x86 manual, III, 4.2)
    unsafe { asm!("pause" :::: "volatile"); }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[inline(always)]
pub fn cpu_relax() {
}
