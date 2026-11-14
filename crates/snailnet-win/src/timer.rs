//! Sub-millisecond sleeps without `timeBeginPeriod` (PLAN.md §6.5).
//!
//! `std::thread::sleep` on Windows rounds up to the system tick (often
//! 15.6 ms), far too coarse for pacing packets. A high-resolution waitable
//! timer (Windows 10 1803+) gives ~0.5 ms.

use std::io;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, INFINITE, SetWaitableTimer,
    TIMER_ALL_ACCESS, WaitForSingleObject,
};

/// Below this, spin instead of sleeping: the timer can't do better.
const SPIN_BELOW: Duration = Duration::from_micros(200);

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

    pub fn sleep(&self, d: Duration) {
        let deadline = Instant::now() + d;
        if d > SPIN_BELOW {
            // Negative = relative, in 100 ns units. Wake a little early and
            // spin the rest for accuracy.
            let ticks = ((d - SPIN_BELOW / 2).as_nanos() / 100) as i64;
            let due = -ticks.max(1);
            // SAFETY: `handle` is a live timer; no completion routine.
            let ok = unsafe { SetWaitableTimer(self.handle, &due, 0, None, std::ptr::null(), 0) };
            if ok != 0 {
                // SAFETY: waiting on our own live handle.
                unsafe { WaitForSingleObject(self.handle, INFINITE) };
            } else {
                std::thread::sleep(d);
            }
        }
        while Instant::now() < deadline {
            std::hint::spin_loop();
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

    #[test]
    fn sleeps_about_one_millisecond() {
        let t = HighResTimer::new().unwrap();
        let mut worst = Duration::ZERO;
        for _ in 0..20 {
            let start = Instant::now();
            t.sleep(Duration::from_millis(1));
            worst = worst.max(start.elapsed());
        }
        assert!(worst >= Duration::from_millis(1));
        // Generous bound for loaded CI machines; a 15.6 ms tick would fail.
        assert!(worst < Duration::from_millis(8), "worst {worst:?}");
    }
}
