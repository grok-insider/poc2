//! Monte Carlo simulation harness.
//!
//! Two modes used by the advisor:
//! - [`run_n_trials`]: run a closure `f(rng)` `n` times, accumulating an
//!   outcome value. Returns mean / variance / standard error.
//! - [`run_until_success`]: run a closure that returns `true` on success
//!   until either success or a max-attempts cap. Returns the attempt count.

use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

/// Simple summary of a Monte Carlo trial set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrialSummary {
    pub trials: u32,
    pub successes: u32,
    pub mean: f64,
    pub variance: f64,
    pub stderr: f64,
}

impl TrialSummary {
    pub fn success_rate(&self) -> f64 {
        if self.trials == 0 {
            return 0.0;
        }
        f64::from(self.successes) / f64::from(self.trials)
    }
}

/// Run `n` Monte Carlo trials. Each trial calls `f(rng)` and returns an
/// `(outcome_value, success_flag)` pair. The summary aggregates the
/// outcome values and counts successes.
///
/// Seed is deterministic — the same `seed` always produces the same summary.
#[must_use]
pub fn run_n_trials<F>(seed: u64, n: u32, mut f: F) -> TrialSummary
where
    F: FnMut(&mut Xoshiro256PlusPlus) -> (f64, bool),
{
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
    let mut sum = 0.0_f64;
    let mut sumsq = 0.0_f64;
    let mut successes = 0_u32;
    for _ in 0..n {
        let (v, ok) = f(&mut rng);
        sum += v;
        sumsq += v * v;
        if ok {
            successes += 1;
        }
    }
    let nf = f64::from(n.max(1));
    let mean = sum / nf;
    let variance = (sumsq / nf - mean * mean).max(0.0);
    let stderr = (variance / nf).sqrt();
    TrialSummary {
        trials: n,
        successes,
        mean,
        variance,
        stderr,
    }
}

/// Run trials until a success-condition is met or `max_attempts` is reached.
/// Returns the number of attempts (0 if zero attempts were made).
#[must_use]
pub fn run_until_success<F>(seed: u64, max_attempts: u32, mut f: F) -> u32
where
    F: FnMut(&mut Xoshiro256PlusPlus, u32) -> bool,
{
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
    for attempt in 1..=max_attempts {
        if f(&mut rng, attempt) {
            return attempt;
        }
    }
    max_attempts
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;

    #[test]
    fn n_trials_deterministic_for_same_seed() {
        let f = |rng: &mut Xoshiro256PlusPlus| {
            let v: f64 = rng.gen();
            (v, v > 0.5)
        };
        let s1 = run_n_trials(42, 1000, f);
        let s2 = run_n_trials(42, 1000, f);
        assert_eq!(s1, s2);
    }

    #[test]
    fn success_rate_approximates_p() {
        // Bernoulli(0.3) over 10000 trials should land near 0.3 ± 0.02.
        let f = |rng: &mut Xoshiro256PlusPlus| {
            let v: f64 = rng.gen();
            (v, v < 0.3)
        };
        let s = run_n_trials(42, 10_000, f);
        assert!((s.success_rate() - 0.3).abs() < 0.02);
    }

    #[test]
    fn mean_of_uniform_is_around_half() {
        let f = |rng: &mut Xoshiro256PlusPlus| {
            let v: f64 = rng.gen();
            (v, true)
        };
        let s = run_n_trials(7, 50_000, f);
        assert!((s.mean - 0.5).abs() < 0.01);
    }

    #[test]
    fn run_until_success_terminates() {
        // p=0.5 should land in 1-3 attempts most of the time. Cap at 50.
        let f = |rng: &mut Xoshiro256PlusPlus, _attempt: u32| {
            let v: f64 = rng.gen();
            v < 0.5
        };
        let attempts = run_until_success(0x00c0_ffee, 50, f);
        assert!(attempts >= 1);
        assert!(attempts <= 50);
    }
}
