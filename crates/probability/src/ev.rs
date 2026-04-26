//! Expected-value rollups, with confidence intervals over weight uncertainty.
//!
//! Per ADR-0004: published mod weights are not authoritative. Each weight
//! observation carries a [`Confidence`] level which translates to a
//! relative interval (Verified ±5%, Community ±25%, Experimental ±50%).
//! When a strategy's success probability depends on a weight, we
//! propagate that interval through the EV computation.

/// Confidence band on a probability — used by the advisor UI to surface
/// uncertainty. The shape mirrors `poc2_data::Confidence` so it can be
/// constructed without that crate dependency in the hot path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvBand {
    pub low: f64,
    pub mid: f64,
    pub high: f64,
}

impl EvBand {
    pub fn point(p: f64) -> Self {
        Self {
            low: p,
            mid: p,
            high: p,
        }
    }

    /// Construct a band by widening `mid` by `relative_interval` in each
    /// direction (clamped to [0, 1]).
    pub fn widen(mid: f64, relative_interval: f64) -> Self {
        let half = mid * relative_interval;
        Self {
            low: (mid - half).max(0.0),
            mid,
            high: (mid + half).min(1.0),
        }
    }

    pub fn width(self) -> f64 {
        self.high - self.low
    }
}

/// Expected attempts to success for a probability band. Wider input bands
/// produce wider attempt-count outputs.
#[must_use]
pub fn expected_attempts_band(p: EvBand) -> Option<EvBand> {
    let invert = |x: f64| -> Option<f64> {
        if x > 0.0 {
            Some(1.0 / x)
        } else {
            None
        }
    };
    let high = invert(p.low)?; // smaller p -> more attempts
    let mid = invert(p.mid)?;
    let low = invert(p.high)?; // larger p -> fewer attempts
    Some(EvBand { low, mid, high })
}

/// Apply a divine-per-trial cost to an attempts band, producing a divine-
/// cost band.
#[must_use]
pub fn cost_band(attempts: EvBand, cost_per_trial: f64) -> EvBand {
    EvBand {
        low: attempts.low * cost_per_trial,
        mid: attempts.mid * cost_per_trial,
        high: attempts.high * cost_per_trial,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn widen_clamps_low_at_zero() {
        let b = EvBand::widen(0.05, 1.5);
        // 0.05 * 1.5 = 0.075; low = max(0.05 - 0.075, 0) = 0.0
        assert!(approx(b.low, 0.0, 1e-9));
        assert!(approx(b.mid, 0.05, 1e-9));
        assert!(approx(b.high, 0.125, 1e-9));
    }

    #[test]
    fn widen_clamps_high_at_one() {
        let b = EvBand::widen(0.95, 0.5);
        // 0.95 * 0.5 = 0.475
        assert!(b.high <= 1.0);
        assert!(approx(b.high, 1.0, 1e-9));
    }

    #[test]
    fn attempts_band_is_inverse_in_p() {
        // p in [0.1, 0.5] -> attempts in [2, 10]
        let p = EvBand {
            low: 0.1,
            mid: 0.25,
            high: 0.5,
        };
        let a = expected_attempts_band(p).unwrap();
        assert!(approx(a.low, 2.0, 1e-9));
        assert!(approx(a.high, 10.0, 1e-9));
        assert!(approx(a.mid, 4.0, 1e-9));
    }

    #[test]
    fn attempts_band_returns_none_for_zero_low() {
        let p = EvBand {
            low: 0.0,
            mid: 0.5,
            high: 1.0,
        };
        assert!(expected_attempts_band(p).is_none());
    }

    #[test]
    fn cost_band_scales_linearly() {
        let a = EvBand {
            low: 2.0,
            mid: 4.0,
            high: 10.0,
        };
        let c = cost_band(a, 1.5);
        assert!(approx(c.low, 3.0, 1e-9));
        assert!(approx(c.mid, 6.0, 1e-9));
        assert!(approx(c.high, 15.0, 1e-9));
    }
}
