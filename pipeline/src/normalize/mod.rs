//! Source-snapshot → Bundle normalization.

pub mod coe_to_bundle;
pub mod fixtures_to_bundle;
pub mod poe2db_to_bundle;
pub mod repoe_to_bundle;

pub use coe_to_bundle::normalize_coe;
pub use fixtures_to_bundle::{flag_essence_target_mods, normalize_fixtures};
pub use poe2db_to_bundle::normalize_poe2db;
pub use repoe_to_bundle::normalize_repoe;
