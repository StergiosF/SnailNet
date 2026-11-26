//! The speed test (PLAN.md §10.1): ~20 s against Cloudflare (the user's
//! choice, 2026-11-18), run by the runner with shaping off so its own
//! traffic isn't held back, and so the ping it takes under load is the
//! line's without SnailNet.
//!
//! 1. Idle ping: five echoes to 1.1.1.1, the median.
//! 2. Download for [`PHASE`] on [`DOWN_STREAMS`] connections, pinging every
//!    half second meanwhile (the loaded ping, median).
//! 3. Upload for [`PHASE`] on [`UP_STREAMS`] connections, pinging the same
//!    way (the loaded upload ping).
//!
//! Then, with shaping back on, the runner downloads once more through
//! SnailNet ([`check`], ~10 s): the ping meanwhile is the "with SnailNet"
//! number of the proof (§11.10).
//!
//! A direction's speed is the 90th percentile of its one-second rates after
//! the first [`WARMUP`] (TCP ramping up).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use snailnet_core::rules::Measured;
use snailnet_win::http::Https;
use snailnet_win::ping::Pinger;

use crate::autorate::PUBLIC_HOST;

const HOST: &str = "speed.cloudflare.com";
const PHASE: Duration = Duration::from_secs(8);
const WARMUP: Duration = Duration::from_secs(2);
const DOWN_STREAMS: usize = 6;
const UP_STREAMS: usize = 4;
/// Bytes per download request; streams ask again until the phase ends.
const DOWN_BYTES: u64 = 25_000_000;
/// Bytes per upload request.
const UP_BYTES: u32 = 10_000_000;
/// How often the byte counter is read.
const SAMPLE: Duration = Duration::from_millis(250);

/// The value at `q` (0–1) of `v`, nearest rank; `None` if empty.
pub fn quantile(v: &[f64], q: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let rank = ((q * s.len() as f64).ceil() as usize).clamp(1, s.len());
    Some(s[rank - 1])
}

/// One-second rates (bits/s) from cumulative byte counts taken every
/// [`SAMPLE`]: each sample against the one a second before it, skipping
/// the warm-up.
pub fn rates(counts: &[(Duration, u64)]) -> Vec<f64> {
    let mut out = Vec::new();
    for (i, &(t, bytes)) in counts.iter().enumerate() {
        if t < WARMUP + Duration::from_secs(1) {
            continue;
        }
        let Some(&(t0, b0)) = counts[..i]
            .iter()
            .rev()
            .find(|(t0, _)| t.saturating_sub(*t0) >= Duration::from_secs(1))
        else {
            continue;
        };
        let secs = (t - t0).as_secs_f64();
        out.push(bytes.saturating_sub(b0) as f64 * 8.0 / secs);
    }
    out
}

/// What one stream does until the deadline or cancellation, adding to the byte counter.
type Work = fn(&Https, Instant, &AtomicBool, &AtomicU64) -> std::io::Result<()>;

/// Sleep up to `d`, waking early if `cancel` is set or `deadline` passes.
fn sleep_cancellable(d: Duration, cancel: &AtomicBool, deadline: Option<Instant>) {
    let end = Instant::now() + d;
    let stop_at = deadline.map_or(end, |dl| end.min(dl));
    while !cancel.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= stop_at {
            break;
        }
        std::thread::sleep(
            stop_at
                .saturating_duration_since(now)
                .min(Duration::from_millis(50)),
        );
    }
}

/// Run `streams` workers until the phase ends, sampling the shared byte
/// counter; the direction's speed in bits/s. `cancel` stops workers & sampling early.
fn phase(https: &Arc<Https>, streams: usize, cancel: &AtomicBool, work: Work) -> Result<f64> {
    let start = Instant::now();
    let deadline = start + PHASE;
    let bytes = AtomicU64::new(0);
    let errors = AtomicU64::new(0);
    let mut counts = vec![(Duration::ZERO, 0)];
    std::thread::scope(|scope| {
        for _ in 0..streams {
            let h = https.as_ref();
            let b = &bytes;
            let e = &errors;
            scope.spawn(move || {
                while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
                    if work(h, deadline, cancel, b).is_err() {
                        e.fetch_add(1, Ordering::Relaxed);
                        sleep_cancellable(Duration::from_millis(200), cancel, Some(deadline));
                    }
                }
            });
        }
        while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
            sleep_cancellable(SAMPLE, cancel, Some(deadline));
            counts.push((start.elapsed(), bytes.load(Ordering::Relaxed)));
        }
    });
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    match quantile(&rates(&counts), 0.9) {
        Some(bps) if bps > 0.0 => Ok(bps),
        _ => bail!(
            "nothing came through ({} failed requests); is the internet up?",
            errors.load(Ordering::Relaxed)
        ),
    }
}

fn download(
    h: &Https,
    deadline: Instant,
    cancel: &AtomicBool,
    bytes: &AtomicU64,
) -> std::io::Result<()> {
    let path = format!("/__down?bytes={DOWN_BYTES}");
    h.download_cancellable(&path, deadline, cancel, &|n| {
        bytes.fetch_add(n, Ordering::Relaxed);
    })
    .map(drop)
}

fn upload(
    h: &Https,
    deadline: Instant,
    cancel: &AtomicBool,
    bytes: &AtomicU64,
) -> std::io::Result<()> {
    h.upload_cancellable("/__up", UP_BYTES, deadline, cancel, &|n| {
        bytes.fetch_add(n, Ordering::Relaxed);
    })
    .map(drop)
}

/// The median of up to `n` pings `gap` apart (stopping at `until` or `cancel`), ms.
fn median_ping(
    p: &mut Pinger,
    n: usize,
    gap: Duration,
    until: Option<Instant>,
    cancel: &AtomicBool,
) -> Option<f64> {
    let mut v = Vec::new();
    for _ in 0..n {
        if cancel.load(Ordering::Relaxed) || until.is_some_and(|u| Instant::now() >= u) {
            break;
        }
        if let Some(rtt) = p.ping(PUBLIC_HOST, Duration::from_millis(1000)) {
            v.push(rtt.as_secs_f64() * 1e3);
        }
        sleep_cancellable(gap, cancel, until);
    }
    quantile(&v, 0.5)
}

/// One loaded phase: `streams` workers for [`PHASE`] while a side thread
/// pings every half second after [`WARMUP`]. The speed (bits/s), the median
/// loaded ping (ms), and the pinger back for the next phase.
fn loaded_phase(
    https: &Arc<Https>,
    streams: usize,
    cancel: &AtomicBool,
    work: Work,
    pinger: Option<Pinger>,
) -> (Result<f64>, Option<f64>, Option<Pinger>) {
    std::thread::scope(|scope| {
        let pings = scope.spawn(move || {
            let mut pinger = pinger;
            let until = Instant::now() + PHASE;
            sleep_cancellable(WARMUP, cancel, Some(until));
            let ms = pinger
                .as_mut()
                .and_then(|p| median_ping(p, 32, Duration::from_millis(500), Some(until), cancel));
            (ms, pinger)
        });
        let bps = phase(https, streams, cancel, work);
        let (ms, pinger) = pings.join().unwrap_or((None, None));
        (bps, ms, pinger)
    })
}

fn client() -> Result<Arc<Https>> {
    // 3 s per-op timeout so cancelling on service stop never blocks for 10 s.
    Ok(Arc::new(Https::with_timeout(
        concat!("SnailNet/", env!("CARGO_PKG_VERSION")),
        HOST,
        3_000,
    )?))
}

/// Run the whole test (blocking, ~20 s). `now`: Unix seconds for the record.
pub fn run(cancel: &AtomicBool, now: u64) -> Result<Measured> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let https = client()?;
    let mut pinger = Pinger::new().ok();
    let idle_ms = pinger
        .as_mut()
        .and_then(|p| median_ping(p, 5, Duration::from_millis(200), None, cancel));
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let (down, loaded_ms, pinger) = loaded_phase(&https, DOWN_STREAMS, cancel, download, pinger);
    let down = down?;
    let (up, loaded_up_ms, _) = loaded_phase(&https, UP_STREAMS, cancel, upload, pinger);
    let up = up?;
    Ok(Measured {
        at: now,
        down_mbps: round1(down / 1e6),
        up_mbps: round1(up / 1e6),
        idle_ms: idle_ms.map(round1),
        loaded_ms: loaded_ms.map(round1),
        loaded_up_ms: loaded_up_ms.map(round1),
        with_ms: None,
    })
}

/// The "with SnailNet" check (§11.10; blocking, ~10 s): the download phase
/// once more, run while SnailNet shapes, so its traffic goes through
/// SnailNet like any app's. The median ping meanwhile, ms; `None` when no
/// ping came back (ICMP blocked).
pub fn check(cancel: &AtomicBool) -> Result<Option<f64>> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let https = client()?;
    let pinger = Pinger::new().ok();
    let (down, ms, _) = loaded_phase(&https, DOWN_STREAMS, cancel, download, pinger);
    down?;
    Ok(ms.map(round1))
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantile_is_nearest_rank() {
        let v: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(quantile(&v, 0.9), Some(9.0));
        assert_eq!(quantile(&v, 0.5), Some(5.0));
        assert_eq!(quantile(&[3.0], 0.9), Some(3.0));
        assert_eq!(quantile(&[], 0.9), None);
    }

    #[test]
    fn rates_skip_the_warmup_and_span_a_second() {
        // 1 MB every 250 ms = 32 Mbps, steady.
        let counts: Vec<_> = (0..=32u32)
            .map(|i| (SAMPLE * i, u64::from(i) * 1_000_000))
            .collect();
        let r = rates(&counts);
        assert!(!r.is_empty());
        for x in &r {
            assert!((x - 32e6).abs() < 1.0, "{x}");
        }
        // Nothing from the first 3 s (2 s warm-up + the first full second).
        let after = counts
            .iter()
            .filter(|(t, _)| *t >= Duration::from_secs(3))
            .count();
        assert_eq!(r.len(), after);
    }

    /// Talks to Cloudflare for ~20 s: run by hand with `--ignored`.
    #[test]
    #[ignore]
    fn measures_the_real_line() {
        let m = run(&AtomicBool::new(false), 0).unwrap();
        println!("{m:?}");
        assert!(m.down_mbps > 0.5 && m.up_mbps > 0.1);
    }
}
