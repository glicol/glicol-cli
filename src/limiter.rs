//! Output safety stage: every sample passes through here on its way to the device,
//! so a NaN, an unstable filter or a typo mid-edit can never reach the speakers as a
//! full-scale (or worse) burst.

use std::sync::atomic::{AtomicBool, Ordering};

/// Below this level samples pass through unchanged.
const KNEE: f32 = 0.8;
/// Output never exceeds this (about -0.45 dBFS).
const CEILING: f32 = 0.95;

static DISABLED: AtomicBool = AtomicBool::new(false);

/// Turns the limiter off for raw output (`--no-limit`).
pub fn disable() {
    DISABLED.store(true, Ordering::Relaxed);
}

/// Non-finite samples become silence; anything above the knee is soft-limited
/// with tanh so it approaches, but never reaches, the ceiling.
pub fn apply(x: f32) -> f32 {
    if DISABLED.load(Ordering::Relaxed) {
        return x;
    }
    if !x.is_finite() {
        return 0.0;
    }
    let mag = x.abs();
    if mag <= KNEE {
        return x;
    }
    let range = CEILING - KNEE;
    (KNEE + range * ((mag - KNEE) / range).tanh()).copysign(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_normal_levels_untouched() {
        for x in [-0.8, -0.5, 0.0, 0.25, 0.8] {
            assert_eq!(apply(x), x);
        }
    }

    #[test]
    fn never_exceeds_ceiling() {
        for x in [0.81, 1.0, 2.0, 1e3, 1e30, f32::MAX] {
            assert!(
                apply(x) <= CEILING && apply(x) > KNEE,
                "{x} -> {}",
                apply(x)
            );
            assert!(
                apply(-x) >= -CEILING && apply(-x) < -KNEE,
                "{} -> {}",
                -x,
                apply(-x)
            );
        }
    }

    #[test]
    fn is_monotonic_and_continuous_at_knee() {
        let xs: Vec<f32> = (0..=2000).map(|i| i as f32 / 500.0).collect();
        for w in xs.windows(2) {
            assert!(apply(w[1]) >= apply(w[0]));
        }
        assert!((apply(KNEE + 1e-4) - KNEE).abs() < 2e-4);
    }

    #[test]
    fn silences_non_finite() {
        for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(apply(x), 0.0);
        }
    }
}
