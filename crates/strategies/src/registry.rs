//! Strategy registry — runtime lookup over a loaded strategy set.

use ahash::AHashMap;
use poc2_engine::ids::ItemClassId;
use poc2_engine::patch::PatchVersion;

use crate::dsl::{Strategy, StrategyId};

/// Patch-versioned, queryable strategy collection.
///
/// Built once at engine startup from a `Vec<Strategy>` (typically loaded
/// from the bundle's strategies directory or from user overrides at
/// `$XDG_CONFIG_HOME/poc2/strategies/`).
#[derive(Debug, Clone, Default)]
pub struct StrategyRegistry {
    by_id: AHashMap<StrategyId, usize>,
    by_class: AHashMap<ItemClassId, Vec<usize>>,
    strategies: Vec<Strategy>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_strategies(strategies: Vec<Strategy>) -> Self {
        let mut by_id = AHashMap::with_capacity(strategies.len());
        let mut by_class: AHashMap<ItemClassId, Vec<usize>> = AHashMap::new();
        for (i, s) in strategies.iter().enumerate() {
            by_id.insert(s.id.clone(), i);
            for c in &s.item_classes {
                by_class.entry(c.clone()).or_default().push(i);
            }
        }
        Self {
            by_id,
            by_class,
            strategies,
        }
    }

    pub fn len(&self) -> usize {
        self.strategies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strategies.is_empty()
    }

    pub fn get(&self, id: &StrategyId) -> Option<&Strategy> {
        self.by_id.get(id).and_then(|i| self.strategies.get(*i))
    }

    /// Iterator over strategies whose `item_classes` contains `class`.
    /// Filtered by patch range against `patch`.
    pub fn for_class<'a>(
        &'a self,
        class: &ItemClassId,
        patch: PatchVersion,
    ) -> impl Iterator<Item = &'a Strategy> + 'a {
        let indices = self
            .by_class
            .get(class)
            .map_or(&[][..], std::vec::Vec::as_slice);
        indices.iter().filter_map(move |i| {
            let s = &self.strategies[*i];
            patch_in_range(s, patch).then_some(s)
        })
    }

    /// Iterator over all strategies (no filter).
    pub fn iter(&self) -> impl Iterator<Item = &Strategy> {
        self.strategies.iter()
    }
}

fn patch_in_range(s: &Strategy, patch: PatchVersion) -> bool {
    s.patch_min.is_none_or(|m| patch >= m) && s.patch_max.is_none_or(|m| patch <= m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{Action, Step, StepId};

    fn mk_strategy(id: &str, classes: &[&str], patch_min: Option<&str>) -> Strategy {
        Strategy {
            id: StrategyId::from(id),
            name: id.into(),
            source: crate::dsl::Source::default(),
            patch_min: patch_min.map(|p| str::parse(p).unwrap()),
            patch_max: None,
            item_classes: classes.iter().map(|c| ItemClassId::from(*c)).collect(),
            attribute_pools: vec![],
            preconditions: vec![],
            target: crate::dsl::Target::default(),
            abandon_criteria: vec![],
            steps: vec![Step {
                id: StepId::from("S1"),
                action: Action::Noop,
                target_check: None,
                on_success: None,
                on_failure: None,
                recovery: smallvec::smallvec![],
                note: None,
            }],
            expected_cost_div: None,
            expected_success_prob: None,
            confidence: crate::dsl::Confidence::default(),
            note: None,
        }
    }

    #[test]
    fn registry_indexes_by_id_and_class() {
        let r = StrategyRegistry::from_strategies(vec![
            mk_strategy("a", &["BodyArmour"], None),
            mk_strategy("b", &["Boots"], None),
            mk_strategy("c", &["BodyArmour", "Helmet"], None),
        ]);
        assert_eq!(r.len(), 3);
        assert!(r.get(&StrategyId::from("a")).is_some());
        let body_armour = ItemClassId::from("BodyArmour");
        let helm = ItemClassId::from("Helmet");
        let p = PatchVersion::PATCH_0_4_0;
        assert_eq!(r.for_class(&body_armour, p).count(), 2);
        assert_eq!(r.for_class(&helm, p).count(), 1);
    }

    #[test]
    fn registry_filters_by_patch_range() {
        let r = StrategyRegistry::from_strategies(vec![
            mk_strategy("legacy_0_3", &["BodyArmour"], Some("0.3.0")),
            mk_strategy("only_0_5", &["BodyArmour"], Some("0.5.0")),
        ]);
        let body_armour = ItemClassId::from("BodyArmour");
        // On 0.4 we see legacy_0_3 (patch_min=0.3.0 satisfied) but NOT only_0_5.
        let strategies_on_0_4: Vec<&Strategy> = r
            .for_class(&body_armour, PatchVersion::PATCH_0_4_0)
            .collect();
        assert_eq!(strategies_on_0_4.len(), 1);
        assert_eq!(strategies_on_0_4[0].id.0, "legacy_0_3");
    }
}
