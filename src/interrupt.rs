use core::arch::asm;
///Contains architecture specific interrupt mask and restore code
/// 
/// 



///
/// Masks all maskable interrupts and returns the previous Interrupt State
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub(crate) fn mask_interrupts() -> bool {

    let mut flags: u64;
    unsafe{
        asm!{
            "pushfw",
            "popw {}",
            "cli",
            out(reg) flags
        }
    }

    //Masks of all Bits except the Interrupt Flag
    if flags & 0x200 > 0 {
        return true;
    }

    false

}

/// Restores the Interrupt State to its previous value
#[cfg(target_arch = "x86_64")]
#[inline(always)]
pub(crate) fn restore_interrupts(interrupts: bool) {
    if interrupts {
        unsafe{
            asm!{
                "sti",
                "nop"   //on x86_64 sti creates a Interrupt Shadow, the NOP contains this Sideeffect to the inline ASM
            }
        }
    }
}