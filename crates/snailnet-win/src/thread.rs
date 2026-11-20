//! Thread priority for the packet path (PLAN.md §6.5).
//!
//! The scheduler and receive threads must run on time even when every core
//! is busy (a build, a game): a scheduler woken late releases less than the
//! line can carry, so SnailNet would make the internet slower than it is
//! without it. Both threads block or nap between packets, so a higher
//! priority costs nothing while the line is quiet.

use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

/// Raise the calling thread to `THREAD_PRIORITY_HIGHEST` (not time
/// critical: it must never starve the PC's input or audio). If Windows
/// refuses, the thread simply keeps its normal priority.
pub fn raise_priority() {
    // SAFETY: the calling thread's pseudo-handle needs no closing and is
    // always valid for SetThreadPriority.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Threading::GetThreadPriority;

    #[test]
    fn raises_the_calling_thread() {
        std::thread::spawn(|| {
            raise_priority();
            // SAFETY: as above.
            let p = unsafe { GetThreadPriority(GetCurrentThread()) };
            assert_eq!(p, THREAD_PRIORITY_HIGHEST);
        })
        .join()
        .unwrap();
    }
}
