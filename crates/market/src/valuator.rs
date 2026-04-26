//! Currency valuator with `DivEquiv(min, expected, max)` triple bounds.
//!
//! Per ADR-0007 + planning, every cost in the advisor is expressed in
//! divine-equivalent units with three bounds: a min (optimistic), an
//! expected (mid), and a max (pessimistic). The advisor's risk slider
//! tunes which bound dominates the ranking decision.
//!
//! Conservative fallback ranges (used when no live price feed is
//! available — see [`Valuator::default`]):
//! - 1 Divine in Exalt: `50..=180` (expected 90)
//! - 1 Divine in Chaos: `3..=30` (expected 8)
//! - 1 Mirror in Divine: `1500..=6000` (expected 2500)
//!
//! Live price pollers (M6+) will overwrite these defaults when online.

use ahash::AHashMap;
use serde::{Deserialize, Serialize};

use poc2_engine::ids::CurrencyId;

/// Three-bound divine-equivalent cost: low / mid / high.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DivEquiv {
    pub min: f64,
    pub expected: f64,
    pub max: f64,
}

impl DivEquiv {
    pub const ZERO: Self = Self {
        min: 0.0,
        expected: 0.0,
        max: 0.0,
    };

    pub const fn point(d: f64) -> Self {
        Self {
            min: d,
            expected: d,
            max: d,
        }
    }

    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            min: self.min + other.min,
            expected: self.expected + other.expected,
            max: self.max + other.max,
        }
    }

    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self {
            min: self.min * k,
            expected: self.expected * k,
            max: self.max * k,
        }
    }

    /// Risk-adjusted point estimate: `expected + risk * (max - expected)`.
    /// `risk = 0.0` → expected; `risk = 1.0` → worst-case max.
    pub fn risk_adjusted(self, risk: f64) -> f64 {
        let r = risk.clamp(0.0, 1.0);
        self.expected + r * (self.max - self.expected)
    }
}

/// Currency valuator. Maintains a per-currency divine-equivalent cost
/// table updated from live price feeds (when online) or from the
/// conservative fallback defaults.
#[derive(Debug, Clone)]
pub struct Valuator {
    table: AHashMap<CurrencyId, DivEquiv>,
}

impl Valuator {
    /// Build a fresh valuator pre-populated with conservative defaults.
    #[allow(clippy::too_many_lines)] // explicit currency price table
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut table = AHashMap::new();
        // Per the planning conservative bands.
        // 1 Exalt = 1 / (50..=180 div) = ~0.0055..=0.020 div
        let ex_in_div = DivEquiv {
            min: 1.0 / 180.0,
            expected: 1.0 / 90.0,
            max: 1.0 / 50.0,
        };
        // 1 Chaos = 1 / (3..=30 div) = 0.033..=0.333 div
        let chaos_in_div = DivEquiv {
            min: 1.0 / 30.0,
            expected: 1.0 / 8.0,
            max: 1.0 / 3.0,
        };
        // 1 Divine = 1 div (the unit).
        let div = DivEquiv::point(1.0);
        // 1 Mirror in divine: 1500..=6000, expected 2500.
        let mirror = DivEquiv {
            min: 1500.0,
            expected: 2500.0,
            max: 6000.0,
        };

        // Basic orbs — prices are illustrative defaults, refreshed from
        // poe2scout / poe.ninja when the live feeders land in M5.3.
        for id in &[
            "OrbOfTransmutation",
            "OrbOfAugmentation",
            "OrbOfAlchemy",
            "RegalOrb",
            "OrbOfAnnulment",
            "ChaosOrb",
        ] {
            table.insert(CurrencyId::from(*id), chaos_in_div);
        }
        for id in &["ExaltedOrb", "VaalOrb"] {
            table.insert(CurrencyId::from(*id), ex_in_div);
        }
        table.insert(CurrencyId::from("DivineOrb"), div);
        table.insert(CurrencyId::from("MirrorOfKalandra"), mirror);

        // Greater / Perfect tier orbs — heuristic 3x / 10x of the base
        // until the live feeder lands.
        for (base, greater, perfect) in [
            (
                "OrbOfTransmutation",
                "GreaterOrbOfTransmutation",
                "PerfectOrbOfTransmutation",
            ),
            (
                "OrbOfAugmentation",
                "GreaterOrbOfAugmentation",
                "PerfectOrbOfAugmentation",
            ),
            ("RegalOrb", "GreaterRegalOrb", "PerfectRegalOrb"),
            ("ExaltedOrb", "GreaterExaltedOrb", "PerfectExaltedOrb"),
            ("ChaosOrb", "GreaterChaosOrb", "PerfectChaosOrb"),
        ] {
            if let Some(d) = table.get(&CurrencyId::from(base)).copied() {
                table.insert(CurrencyId::from(greater), d.scale(3.0));
                table.insert(CurrencyId::from(perfect), d.scale(10.0));
            }
        }

        // Specialty currencies — placeholder defaults.
        table.insert(
            CurrencyId::from("FracturingOrb"),
            DivEquiv {
                min: 30.0,
                expected: 50.0,
                max: 150.0,
            },
        );
        table.insert(
            CurrencyId::from("HinekorasLock"),
            DivEquiv {
                min: 30.0,
                expected: 60.0,
                max: 200.0,
            },
        );
        // Bones (preserved/ancient generic bucket — refined per subtype later).
        table.insert(
            CurrencyId::from("PreservedRib"),
            DivEquiv {
                min: 0.5,
                expected: 1.5,
                max: 5.0,
            },
        );
        table.insert(
            CurrencyId::from("AncientRib"),
            DivEquiv {
                min: 5.0,
                expected: 15.0,
                max: 50.0,
            },
        );
        // Common omens — illustrative pricing only.
        for id in &[
            "OmenOfSinistralExaltation",
            "OmenOfDextralExaltation",
            "OmenOfGreaterExaltation",
            "OmenOfWhittling",
            "OmenOfCorruption",
            "OmenOfSinistralCrystallisation",
            "OmenOfDextralCrystallisation",
            "OmenOfSinistralNecromancy",
            "OmenOfDextralNecromancy",
            "OmenOfAbyssalEchoes",
        ] {
            table.insert(
                CurrencyId::from(*id),
                DivEquiv {
                    min: 1.0,
                    expected: 5.0,
                    max: 30.0,
                },
            );
        }
        table.insert(
            CurrencyId::from("OmenOfLight"),
            DivEquiv {
                min: 50.0,
                expected: 100.0,
                max: 200.0,
            },
        );
        table.insert(
            CurrencyId::from("OmenOfSanctification"),
            DivEquiv {
                min: 200.0,
                expected: 400.0,
                max: 800.0,
            },
        );

        Self { table }
    }

    /// Look up a currency's divine-equivalent cost. Returns `None` for
    /// currencies the valuator hasn't seen — the caller decides whether
    /// to treat as zero, error, or use a conservative ceiling.
    pub fn get(&self, id: &CurrencyId) -> Option<DivEquiv> {
        self.table.get(id).copied()
    }

    /// Insert or overwrite a currency's price.
    pub fn set(&mut self, id: CurrencyId, value: DivEquiv) {
        self.table.insert(id, value);
    }

    /// Number of priced currencies.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }
}

impl Default for Valuator {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn default_valuator_has_basic_orbs_priced() {
        let v = Valuator::default();
        assert!(v.get(&CurrencyId::from("OrbOfTransmutation")).is_some());
        assert!(v.get(&CurrencyId::from("DivineOrb")).is_some());
        assert!(v.get(&CurrencyId::from("MirrorOfKalandra")).is_some());
        assert!(v.get(&CurrencyId::from("PerfectExaltedOrb")).is_some());
    }

    #[test]
    fn divine_costs_one_div() {
        let v = Valuator::default();
        let div = v.get(&CurrencyId::from("DivineOrb")).unwrap();
        assert!(approx(div.expected, 1.0, 1e-9));
    }

    #[test]
    fn perfect_orbs_cost_more_than_base() {
        let v = Valuator::default();
        let base = v.get(&CurrencyId::from("ExaltedOrb")).unwrap();
        let perfect = v.get(&CurrencyId::from("PerfectExaltedOrb")).unwrap();
        assert!(perfect.expected > base.expected);
    }

    #[test]
    fn risk_adjusted_lerp_works() {
        let d = DivEquiv {
            min: 1.0,
            expected: 5.0,
            max: 100.0,
        };
        assert!(approx(d.risk_adjusted(0.0), 5.0, 1e-9));
        assert!(approx(d.risk_adjusted(1.0), 100.0, 1e-9));
        assert!(approx(d.risk_adjusted(0.5), 52.5, 1e-9));
    }

    #[test]
    fn divequiv_arithmetic() {
        let a = DivEquiv {
            min: 1.0,
            expected: 2.0,
            max: 4.0,
        };
        let b = DivEquiv::point(0.5);
        let s = a.plus(b);
        assert!(approx(s.min, 1.5, 1e-9));
        assert!(approx(s.expected, 2.5, 1e-9));
        assert!(approx(s.max, 4.5, 1e-9));

        let s2 = a.scale(2.0);
        assert!(approx(s2.min, 2.0, 1e-9));
        assert!(approx(s2.max, 8.0, 1e-9));
    }

    #[test]
    fn user_supplied_value_overrides_default() {
        let mut v = Valuator::default();
        v.set(
            CurrencyId::from("CustomCurrency"),
            DivEquiv {
                min: 10.0,
                expected: 50.0,
                max: 100.0,
            },
        );
        let d = v.get(&CurrencyId::from("CustomCurrency")).unwrap();
        assert!(approx(d.expected, 50.0, 1e-9));
    }
}
