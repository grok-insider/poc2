//! Omen system.
//!
//! Stub for M1.
//! Full implementation in M2 covers all crafting omens (currency-targeting and
//! abyss-targeting). Each omen declares:
//! - `targets_currency` — which currency type it modifies
//! - `effect` — how the currency's outcome distribution changes when active
//! - `patch_range` — when the omen is in the active drop pool (e.g.,
//!   Homogenising Exaltation has `patch_max = 0.3.x` per planning docs)
