//! `sim` command — bulk Monte-Carlo of a single advisor action.
//!
//! Ported from the desktop `run_n_trials` Tauri command. Pure compute: runs
//! `n` independent deterministic [`simulate`](poc2_advisor::simulate) trials of
//! one action against an item, aggregating success rate (with stderr), a
//! change-count histogram, and an expected divine-equivalent cost.
//!
//! Tauri-specific concerns (`tauri::State`, RwLock/Mutex locking, AppHandle)
//! are dropped — the caller passes the engine objects by reference.

use poc2_advisor::AdvisorAction;
use poc2_engine::currency::CurrencyResolver;
use poc2_engine::item::Item;
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::PatchVersion;
use poc2_engine::ModRegistry;
use poc2_market::Valuator;
use serde::Serialize;
use std::collections::BTreeMap;

/// Aggregate statistics over `n` Monte-Carlo trials of one action.
///
/// Serde field names mirror the desktop `TrialDistribution` (the web TS
/// contract depends on them — do not rename).
#[derive(Debug, Serialize)]
pub struct TrialDistribution {
    /// Number of trials actually run.
    pub n_trials: u32,
    /// Fraction of trials where the action succeeded.
    pub success_rate: f64,
    /// sqrt(p(1-p)/n) — confidence on the rate estimate.
    pub success_rate_stderr: f64,
    /// Mean number of mod-affecting changes per trial.
    pub mean_change_count: f64,
    /// Histogram of `change_count` values: `bucket -> count`.
    pub change_count_histogram: BTreeMap<u32, u32>,
    /// Estimated divine-equivalent cost per trial (constant — we use
    /// the action's cost band's expected value).
    pub cost_per_trial_div: f64,
    /// Estimated total cost across n_trials at the expected per-trial
    /// cost.
    pub total_cost_div_expected: f64,
}

/// Run `n_trials` independent Monte-Carlo trials of `action` against `item`.
///
/// Each trial is seeded deterministically from `seed` (so the whole batch is
/// reproducible). `n_trials` is clamped to `[1, 10_000]`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn run_n_trials(
    registry: &ModRegistry,
    resolver: &dyn CurrencyResolver,
    valuator: &Valuator,
    item: &Item,
    action: &AdvisorAction,
    n_trials: u32,
    seed: u64,
    patch: PatchVersion,
) -> TrialDistribution {
    let n = n_trials.clamp(1, 10_000);
    let omens = OmenSet::new();

    let mut successes = 0_u32;
    let mut total_change_count = 0_u32;
    let mut histogram: BTreeMap<u32, u32> = BTreeMap::new();
    for i in 0..n {
        let seed = seed.wrapping_add(u64::from(i).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let outcome = poc2_advisor::simulate(item, action, &omens, registry, resolver, patch, seed);
        if outcome.success {
            successes += 1;
        }
        total_change_count = total_change_count.saturating_add(outcome.change_count);
        *histogram.entry(outcome.change_count).or_insert(0) += 1;
    }

    let n_f = f64::from(n);
    let p = f64::from(successes) / n_f;
    let stderr = if n <= 1 {
        0.0
    } else {
        (p * (1.0 - p) / n_f).sqrt()
    };
    let cost_per_trial = poc2_advisor::action_cost(action, valuator).expected;

    TrialDistribution {
        n_trials: n,
        success_rate: p,
        success_rate_stderr: stderr,
        mean_change_count: f64::from(total_change_count) / n_f,
        change_count_histogram: histogram,
        cost_per_trial_div: cost_per_trial,
        total_cost_div_expected: cost_per_trial * n_f,
    }
}
