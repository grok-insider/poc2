//! Concept map: raw stat-ids → our semantic concept taxonomy.
//!
//! Built once by the pipeline at bundle build time. The engine's
//! mod_analyzer uses it to compute each mod's `concept_set`, which drives
//! hybrid classification and concept-based target matching.
//!
//! Example mappings (illustrative — full set lives in `concept_map.toml`):
//!
//! ```text
//! local_energy_shield_+%      → EnergyShield
//! base_maximum_energy_shield  → EnergyShield
//! base_maximum_life           → Life
//! base_maximum_mana           → Mana
//! base_fire_damage_resistance → FireResistance
//! cold_damage_resistance      → ColdResistance
//! attack_speed_+%             → AttackSpeed
//! ```

use ahash::AHashMap;
use poc2_engine::{ConceptId, StatId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptDefinition {
    pub id: ConceptId,
    pub display_name: String,
    /// Higher-level grouping for UI ("Defence", "Damage", "Resource", ...).
    pub family: String,
}

/// One row in the concept map: stat → concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptMapEntry {
    pub stat_id: StatId,
    pub concept_id: ConceptId,
}

/// Concept map — `stat_id → concept_id`.
///
/// In serialized form it's a `Vec<ConceptMapEntry>` for stable JSON ordering.
/// In memory we expose an `AHashMap` view via [`ConceptMap::lookup`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConceptMap(pub Vec<ConceptMapEntry>);

impl ConceptMap {
    /// Build a `stat_id → concept_id` lookup. O(n); call once at bundle load.
    pub fn lookup(&self) -> AHashMap<StatId, ConceptId> {
        self.0
            .iter()
            .map(|e| (e.stat_id.clone(), e.concept_id.clone()))
            .collect()
    }
}
