//! Sub-millisecond sleeps without `timeBeginPeriod` (PLAN.md §6.5).
//!
//! `std::thread::sleep` on Windows rounds up to the system tick (often
//! 15.6 ms), far too coarse for pacing packets. A high-resolution waitable
//! timer (Windows 10 1803+) gives ~0.5 ms.
//!
//! It never spins, and naps at least [`MIN_NAP`] (PLAN.md §6.5, §20,
//! measured 2026-11-18). Most of the shaper's CPU is kernel time per
//! WinDivert send call, so fewer, fuller sends are what save CPU; spinning
//! for microsecond accuracy buys nothing, because the scheduler's root
//! bucket holds 10 ms of credit (`ROOT_BURST_MS`) and a wake-up late just releases
//! what came due meanwhile, at the same rate.

use std::io;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE, SetWaitableTimer,
    TIMER_ALL_ACCESS, WaitForSingleObject,
};

/// Shorter naps are stretched to this, so a busy shaper wakes about a
/// thousand times a second and each send carries several packets. With
/// the timer's lateness it stays well inside the root bucket's 10 ms of credit.
pub const MIN_NAP: Duration = Duration::from_millis(1);

pub struct HighResTimer {
    handle: HANDLE,
}

// SAFETY: a waitable timer handle may be used from any thread.
unsafe impl Send for HighResTimer {}

impl HighResTimer {
    pub fn new() -> io::Result<Self> {
        // SAFETY: null attributes and name are allowed.
        let handle = unsafe {
            CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    /// Sleep at least `d` (at least [`MIN_NAP`]); the timer may run a
    /// fraction of a millisecond over.
    pub fn sleep(&self, d: Duration) {
        let d = d.max(MIN_NAP);
        // Negative = relative, in 100 ns units.
        let due = -((d.as_nanos() / 100) as i64).max(1);
        // SAFETY: `handle` is a live timer; no completion routine.
        let ok = unsafe { SetWaitableTimer(self.handle, &due, 0, None, std::ptr::null(), 0) };
        if ok != 0 {
            // SAFETY: waiting on our own live handle.
            unsafe { WaitForSingleObject(self.handle, INFINITE) };
        } else {
            std::thread::sleep(d);
        }
    }
}

impl Drop for HighResTimer {
    fn drop(&mut self) {
        // SAFETY: closed exactly once.
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Instant;

    #[test]
    fn sleeps_about_one_millisecond() {
        let t = HighResTimer::new().unwrap();
        let (mut best, mut worst) = (Duration::MAX, Duration::ZERO);
        for _ in 0..20 {
            let start = Instant::now();
            t.sleep(Duration::from_millis(1));
            let took = start.elapsed();
            (best, worst) = (best.min(took), worst.max(took));
        }
        // The timer's own granularity allows a hair early.
        assert!(best >= Duration::from_micros(900), "best {best:?}");
        // Generous bound for loaded CI machines; a 15.6 ms tick would fail.
        assert!(worst < Duration::from_millis(8), "worst {worst:?}");
    }

    #[test]
    fn short_naps_are_stretched_not_spun() {
        let t = HighResTimer::new().unwrap();
        let start = Instant::now();
        for _ in 0..20 {
            t.sleep(Duration::from_micros(10));
        }
        // 20 naps of at least MIN_NAP (a spin would take ~0.2 ms in all).
        assert!(start.elapsed() >= MIN_NAP * 18, "{:?}", start.elapsed());
    }
}
