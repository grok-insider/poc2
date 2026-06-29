//! Source-snapshot → Bundle normalization.

pub mod alloy_fixups;
pub mod coe_to_bundle;
pub mod fixtures_to_bundle;
pub mod genesis_to_bundle;
pub mod poe2db_to_bundle;
pub mod repoe_to_bundle;
pub mod tiers;

pub use alloy_fixups::apply_alloy_fixups;
pub use coe_to_bundle::normalize_coe;
pub use fixtures_to_bundle::{flag_essence_target_mods, normalize_fixtures};
pub use genesis_to_bundle::normalize_genesis;
pub use poe2db_to_bundle::normalize_poe2db;
pub use repoe_to_bundle::normalize_repoe;
pub use tiers::assign_tier_ordinals;
