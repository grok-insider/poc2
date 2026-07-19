//! `eligible` command — enumerate eligible/blocked mods for a given
//! (item, affix) slot.
//!
//! Ported from the Tauri desktop `eligible_mods` command. For the given
//! item and affix slot it enumerates every explicit mod the bundle says
//! could roll on this base + ilvl, plus mods that are blocked only by a
//! Greater/Perfect "min required level" floor (so the UI can grey them out
//! with an explanation). Pure compute: takes the engine registries by
//! reference and returns the serialisable view directly — no Tauri state,
//! clipboard, disk, or asset concerns.

use poc2_engine::ids::{ItemClassId, TagId};
use poc2_engine::item::{AffixType, Item};
use poc2_engine::mods::{ModDefinition, ModFlags, ModKind};
use poc2_engine::patch::PatchVersion;
use poc2_engine::{BaseRegistry, ModRegistry};
use serde::{Deserialize, Serialize};

/// Spawn weight of a mod **on this base**: the tag-intersection
/// (leftmost-tag-wins) weight when the base's tags are known, else the raw
/// sum (legacy fallback when the base wasn't resolved). A result of `0` means
/// this base's attribute variant cannot roll the mod.
fn base_weight(m: &ModDefinition, base_tags: &[TagId]) -> u32 {
    if base_tags.is_empty() {
        m.spawn_weights.iter().map(|sw| sw.weight).sum()
    } else {
        m.spawn_weight_for_tags(base_tags).unwrap_or(0)
    }
}

/// Which affix slot(s) to enumerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffixSlotFilter {
    Prefix,
    Suffix,
    Either,
}

impl AffixSlotFilter {
    fn matches(self, ty: AffixType) -> bool {
        match self {
            Self::Prefix => matches!(ty, AffixType::Prefix),
            Self::Suffix => matches!(ty, AffixType::Suffix),
            Self::Either => matches!(ty, AffixType::Prefix | AffixType::Suffix),
        }
    }
}

pub const fn default_affix_slot() -> AffixSlotFilter {
    AffixSlotFilter::Either
}

#[derive(Debug, Serialize)]
pub struct EligibleModView {
    pub mod_id: String,
    pub name: Option<String>,
    pub mod_group: String,
    pub affix_type: String,
    pub kind: String,
    /// Concept ids this mod produces, e.g. ["EnergyShield"].
    pub concepts: Vec<String>,
    /// Tags (e.g. "boots", "movement").
    pub tags: Vec<String>,
    /// Tier index within the mod-group ladder (1 = highest required level).
    pub tier_index: u32,
    /// Total tiers for this mod-group on this base.
    pub tier_count: u32,
    pub required_level: u32,
    /// Eligible right now (passes class+ilvl+groups+patch+positive weight).
    pub eligible_now: bool,
    /// Blocked by `min_required_level` even though otherwise eligible.
    pub blocked_by_min_level: bool,
    /// Already present on the item (mod-group exclusivity).
    pub blocked_by_group: bool,
    /// Sum of spawn weights for tags relevant on this item.
    pub weight: u32,
    /// Probability share among the eligible-now set.
    pub weight_share: f64,
    pub text_template: Option<String>,
    /// Stat ranges `(stat_id, min, max)`, in mod's own order.
    pub stats: Vec<EligibleStatView>,
    pub is_hybrid: bool,
    pub is_essence_only: bool,
    pub is_desecrated_only: bool,
    pub is_local: bool,
}

#[derive(Debug, Serialize)]
pub struct EligibleStatView {
    pub stat_id: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
pub struct EligibleModsResponse {
    /// Item class derived from the input item.
    pub item_class: String,
    /// Whether the bundle has any mods registered for this item-class+affix.
    /// `false` means the UI should show a "no_data_for_class" notice.
    pub data_available: bool,
    pub affix: String,
    /// Patch the registry was loaded for.
    pub patch: String,
    pub mods: Vec<EligibleModView>,
}

/// Enumerate eligible/blocked mods for `(item, affix)`.
///
/// `min_required_level` is a floor (e.g. Perfect Transmute = 70): mods below
/// it are returned but flagged `blocked_by_min_level`.
///
/// The item class and the base's attribute-variant tags are resolved from
/// `base_registry`, so the returned pool reflects what THIS base (str/dex/int)
/// can actually roll — mods whose tag-intersection weight is `0` are excluded.
pub fn eligible_mods(
    registry: &ModRegistry,
    base_registry: &BaseRegistry,
    item: &Item,
    affix: AffixSlotFilter,
    min_required_level: u32,
    patch: PatchVersion,
) -> EligibleModsResponse {
    let class = base_registry
        .class_of(&item.base)
        .cloned()
        .unwrap_or_else(|| ItemClassId::from(item.base.as_str()));
    let base_tags = base_registry.tags_of(&item.base).to_vec();

    // Collect occupied groups already on the item (from any affix slot).
    let mut occupied_groups: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in item.prefixes.iter().chain(item.suffixes.iter()) {
        if let Some(g) = registry.group_of(&m.mod_id) {
            occupied_groups.insert(g.as_str().to_string());
        }
    }

    let affix_label = match affix {
        AffixSlotFilter::Prefix => "prefix",
        AffixSlotFilter::Suffix => "suffix",
        AffixSlotFilter::Either => "either",
    };

    // Build a candidate index: all mods for the class on the relevant affix.
    let mut indices: Vec<_> = Vec::new();
    if affix.matches(AffixType::Prefix) {
        indices.extend(
            registry
                .for_class_affix(&class, AffixType::Prefix)
                .iter()
                .copied(),
        );
    }
    if affix.matches(AffixType::Suffix) {
        indices.extend(
            registry
                .for_class_affix(&class, AffixType::Suffix)
                .iter()
                .copied(),
        );
    }

    if indices.is_empty() {
        return EligibleModsResponse {
            item_class: class.as_str().to_string(),
            data_available: false,
            affix: affix_label.to_string(),
            patch: format!("{patch}"),
            mods: Vec::new(),
        };
    }

    // Group counts for tier_index/tier_count assignment. Tier 1 = highest
    // required_level within group.
    let mut group_levels: std::collections::HashMap<String, Vec<u32>> =
        std::collections::HashMap::new();
    for &idx in &indices {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        // Exclude mods this base's attribute variant cannot roll (tag weight 0).
        if base_weight(m, &base_tags) == 0 {
            continue;
        }
        group_levels
            .entry(m.mod_group.0.as_str().to_string())
            .or_default()
            .push(m.required_level);
    }
    for v in group_levels.values_mut() {
        v.sort_unstable_by(|a, b| b.cmp(a)); // descending: highest required_level first = T1
        v.dedup();
    }

    // First pass: build raw list and remember the eligible-now subset's total weight.
    let mut raw: Vec<EligibleModView> = Vec::new();
    let mut eligible_total_weight: u64 = 0;

    for idx in indices {
        let Some(m) = registry.at(idx) else { continue };
        if m.kind != ModKind::Explicit {
            continue;
        }
        if !m.patch_range.contains(patch) {
            continue;
        }
        let group_id = m.mod_group.0.as_str().to_string();
        let weight: u32 = base_weight(m, &base_tags);
        if weight == 0 {
            continue;
        }
        let blocked_by_group = occupied_groups.contains(&group_id);
        let blocked_by_min = m.required_level < min_required_level;
        let blocked_by_ilvl = m.required_level > item.ilvl;
        let eligible_now = !blocked_by_group && !blocked_by_min && !blocked_by_ilvl;

        if eligible_now {
            eligible_total_weight = eligible_total_weight.saturating_add(u64::from(weight));
        }

        let levels = group_levels.get(&group_id);
        let tier_count = levels.map(|v| v.len() as u32).unwrap_or(1);
        let tier_index = levels
            .and_then(|v| v.iter().position(|l| *l == m.required_level))
            .map(|p| (p + 1) as u32)
            .unwrap_or(1);

        raw.push(EligibleModView {
            mod_id: m.id.as_str().to_string(),
            name: m.name.clone(),
            mod_group: group_id,
            affix_type: match m.affix_type {
                AffixType::Prefix => "prefix".into(),
                AffixType::Suffix => "suffix".into(),
                AffixType::Implicit => "implicit".into(),
                AffixType::Enchantment => "enchantment".into(),
            },
            kind: format!("{:?}", m.kind).to_ascii_lowercase(),
            concepts: m
                .concept_set
                .iter()
                .map(|c| c.as_str().to_string())
                .collect(),
            tags: m.tags.iter().map(|t| t.as_str().to_string()).collect(),
            tier_index,
            tier_count,
            required_level: m.required_level,
            eligible_now,
            blocked_by_min_level: blocked_by_min && !blocked_by_ilvl && !blocked_by_group,
            blocked_by_group,
            weight,
            weight_share: 0.0,
            text_template: m.text_template.clone(),
            stats: m
                .stats
                .iter()
                .map(|s| EligibleStatView {
                    stat_id: s.stat_id.as_str().to_string(),
                    min: s.min,
                    max: s.max,
                })
                .collect(),
            is_hybrid: m.flags.contains(ModFlags::HYBRID),
            is_essence_only: m.flags.contains(ModFlags::ESSENCE_ONLY),
            is_desecrated_only: m.flags.contains(ModFlags::DESECRATED_ONLY),
            is_local: m.flags.contains(ModFlags::LOCAL),
        });
    }

    if eligible_total_weight > 0 {
        for view in &mut raw {
            if view.eligible_now {
                view.weight_share = view.weight as f64 / eligible_total_weight as f64;
            }
        }
    }

    // Sort: eligible first, then by tier_index asc (T1 first), then weight desc.
    raw.sort_by(|a, b| {
        b.eligible_now
            .cmp(&a.eligible_now)
            .then(a.tier_index.cmp(&b.tier_index))
            .then(b.weight.cmp(&a.weight))
            .then(a.mod_id.cmp(&b.mod_id))
    });

    EligibleModsResponse {
        item_class: class.as_str().to_string(),
        data_available: true,
        affix: affix_label.to_string(),
        patch: format!("{patch}"),
        mods: raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::base::{BaseType, InventorySize, ReleaseState};
    use poc2_engine::ids::{BaseTypeId, ConceptId, ModGroupId, ModId, StatId};
    use poc2_engine::item::{QualityKind, Rarity};
    use poc2_engine::item_class::AttributePool;
    use poc2_engine::mods::{ModDomain, ModGroup, ModStat, SpawnWeight};
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    fn mk_tagged_mod(id: &str, weight_tag: &str) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: Some(id.to_string()),
            mod_group: ModGroup(ModGroupId::from(format!("g-{id}"))),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from(weight_tag),
                weight: 100
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("s"),
                min: 0.0,
                max: 1.0
            }],
            required_level: 1,
            tier: Some(1),
            allowed_item_classes: smallvec![ItemClassId::from("Focus")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: Some("x".into()),
        }
    }

    fn mk_base(id: &str, tags: &[&str]) -> BaseType {
        BaseType {
            id: BaseTypeId::from(id),
            name: id.to_string(),
            item_class: ItemClassId::from("Focus"),
            attribute_pool: AttributePool::Int,
            drop_level: 1,
            tags: tags.iter().map(|t| TagId::from(*t)).collect(),
            implicits: smallvec![],
            inventory: InventorySize {
                width: 1,
                height: 1,
            },
            release_state: ReleaseState::Released,
            patch_range: PatchRange::ALL,
        }
    }

    fn empty_item(base: &str) -> Item {
        Item {
            base: BaseTypeId::from(base),
            ilvl: 80,
            rarity: Rarity::Normal,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: smallvec![],
            suffixes: smallvec![],
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    #[test]
    fn base_attribute_variant_gates_the_pool() {
        let registry = ModRegistry::from_mods(
            vec![
                mk_tagged_mod("EsMod", "int_armour"),
                mk_tagged_mod("ArmourMod", "str_armour"),
            ],
            vec![],
        );
        let base_registry = BaseRegistry::from_bases(vec![
            mk_base("IntBase", &["focus", "int_armour"]),
            mk_base("StrBase", &["focus", "str_armour"]),
        ]);

        let int_resp = eligible_mods(
            &registry,
            &base_registry,
            &empty_item("IntBase"),
            AffixSlotFilter::Prefix,
            0,
            PatchVersion::PATCH_0_5_0,
        );
        let int_ids: Vec<&str> = int_resp.mods.iter().map(|m| m.mod_id.as_str()).collect();
        assert!(
            int_ids.contains(&"EsMod"),
            "int base should roll the ES mod"
        );
        assert!(
            !int_ids.contains(&"ArmourMod"),
            "int base must NOT roll the str-only armour mod"
        );

        let str_resp = eligible_mods(
            &registry,
            &base_registry,
            &empty_item("StrBase"),
            AffixSlotFilter::Prefix,
            0,
            PatchVersion::PATCH_0_5_0,
        );
        let str_ids: Vec<&str> = str_resp.mods.iter().map(|m| m.mod_id.as_str()).collect();
        assert!(str_ids.contains(&"ArmourMod"));
        assert!(!str_ids.contains(&"EsMod"));
    }
}
