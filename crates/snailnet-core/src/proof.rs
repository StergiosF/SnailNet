//! The proof (PLAN.md §11.10): how much ping rises when the line is full,
//! as a grade from A to F.
//!
//! The scale is the one the well-known bufferbloat tests use (Waveform's),
//! so people can compare: rise = loaded − idle ping, **A ≤ 30 ms, B ≤ 60,
//! C ≤ 200, D ≤ 400, F above**. No A+: the plain scale reads more clearly.

use serde::{Deserialize, Serialize};

/// The highest rise, ms, that still earns an A.
pub const A_MAX_MS: f64 = 30.0;
/// The highest rise, ms, that still earns a B.
pub const B_MAX_MS: f64 = 60.0;
/// The highest rise, ms, that still earns a C.
pub const C_MAX_MS: f64 = 200.0;
/// The highest rise, ms, that still earns a D; above it is F.
pub const D_MAX_MS: f64 = 400.0;

/// How well the line keeps ping low while it's full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

/// The grade for a ping rise of `rise_ms`.
pub fn grade(rise_ms: f64) -> Grade {
    if rise_ms <= A_MAX_MS {
        Grade::A
    } else if rise_ms <= B_MAX_MS {
        Grade::B
    } else if rise_ms <= C_MAX_MS {
        Grade::C
    } else if rise_ms <= D_MAX_MS {
        Grade::D
    } else {
        Grade::F
    }
}

/// How far the loaded ping sits above the idle one, ms; `None` unless both
/// were measured. Never negative: a loaded ping below the idle one (noise)
/// is a rise of 0.
pub fn rise(idle_ms: Option<f64>, loaded_ms: Option<f64>) -> Option<f64> {
    let (idle, loaded) = (idle_ms?, loaded_ms?);
    if !idle.is_finite() || !loaded.is_finite() {
        return None;
    }
    Some((loaded - idle).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_edge_belongs_to_the_better_grade() {
        for (rise, g) in [
            (0.0, Grade::A),
            (30.0, Grade::A),
            (30.1, Grade::B),
            (60.0, Grade::B),
            (60.1, Grade::C),
            (200.0, Grade::C),
            (200.1, Grade::D),
            (400.0, Grade::D),
            (400.1, Grade::F),
            (5000.0, Grade::F),
        ] {
            assert_eq!(grade(rise), g, "{rise} ms");
        }
    }

    #[test]
    fn rise_needs_both_and_is_never_negative() {
        assert_eq!(rise(Some(12.0), Some(142.0)), Some(130.0));
        assert_eq!(rise(Some(20.0), Some(18.0)), Some(0.0));
        assert_eq!(rise(None, Some(18.0)), None);
        assert_eq!(rise(Some(20.0), None), None);
        assert_eq!(rise(Some(f64::NAN), Some(18.0)), None);
    }
}
