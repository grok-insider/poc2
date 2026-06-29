//! `record_outcome` command — ported from the Tauri desktop app.
//!
//! Applies a user-chosen mod outcome to an in-memory [`Item`]. This is how
//! the UI integrates "I just used Perfect Transmute and rolled X" into the
//! session's item state without going through random sampling.
//!
//! The input/response shapes mirror the desktop's `RecordOutcomeArgs`,
//! `OutcomeKind`, `RerolledMod` and `RecordOutcomeResponse` verbatim so the
//! web TS contract (`apps/web/lib/types.ts`) does not have to change.

use poc2_engine::ids::ModId;
use poc2_engine::item::{AffixType, Item, ModRoll, Rarity};
use poc2_engine::mods::ModDefinition;
use poc2_engine::registry::ModRegistry;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------
// Input types (Deserialize) — keep serde shapes identical to desktop.
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RecordOutcomeArgs {
    pub item: Item,
    pub outcome: OutcomeKind,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutcomeKind {
    /// Add a mod that the user picked from the eligible-mods list.
    AddMod {
        mod_id: String,
        /// 0..=1 normalized roll along the mod's stat range. None = midpoint.
        #[serde(default)]
        roll: Option<f64>,
        /// Currency that produced this mod (informational, used for rarity
        /// transitions like Normal→Magic on Transmute).
        #[serde(default)]
        currency: Option<String>,
    },
    /// Remove a mod by (affix, index) — used for Annul/Chaos.
    RemoveMod { affix: String, index: usize },
    /// Replace a mod (Chaos): remove `(affix, index)` then add `mod_id`.
    ReplaceMod {
        remove_affix: String,
        remove_index: usize,
        add_mod_id: String,
        #[serde(default)]
        roll: Option<f64>,
    },
    /// Reroll the values of one or more existing mods within their current
    /// tier ranges — used for Divine Orb (and its omen variants). The
    /// player's rolled numbers come in absolute (not normalized) form
    /// because that is what the in-game tooltip shows.
    ///
    /// `sanctify == true` switches the value bounds from `[min, max]` to
    /// `[min × 0.8, max × 1.2]` (per Omen of Sanctification mechanics) and
    /// sets `Item.sanctified = true`. Sanctification requires Rare rarity.
    RerollValues {
        rolls: Vec<RerolledMod>,
        #[serde(default)]
        sanctify: bool,
    },
    /// Manual rarity bump (no mod change). Used when the engine doesn't
    /// know what to roll for the currency yet.
    SetRarity { rarity: String },
}

/// One mod's worth of rerolled values. `slot` is `"implicit"`, `"prefix"`,
/// or `"suffix"`; `index` is the slot-local index. `values` carries one
/// absolute number per stat in the parent mod definition's `stats` array,
/// in the same order.
#[derive(Debug, Deserialize)]
pub struct RerolledMod {
    pub slot: String,
    pub index: usize,
    pub values: Vec<f64>,
}

// ---------------------------------------------------------------------
// Response type (Serialize) — keep serde field names identical to desktop.
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RecordOutcomeResponse {
    pub item: Item,
    pub change: String,
    pub explanation: String,
}

// ---------------------------------------------------------------------
// Command — pure compute, decoupled from Tauri.
// ---------------------------------------------------------------------

/// Apply a [`RecordOutcomeArgs`] to its embedded item and return the mutated
/// item plus a change summary.
///
/// Ported from the desktop `record_outcome` Tauri command. The only
/// behavioural change is that the mod registry comes in by reference instead
/// of being cloned out of the bundle rwlock.
pub fn record_outcome(
    registry: &ModRegistry,
    args: RecordOutcomeArgs,
) -> Result<RecordOutcomeResponse, String> {
    let mut item = args.item;

    match args.outcome {
        OutcomeKind::AddMod {
            mod_id,
            roll,
            currency,
        } => {
            let mid = ModId::from(mod_id.clone());
            let def = registry
                .get(&mid)
                .ok_or_else(|| format!("unknown mod id: {mod_id}"))?;
            // Validate ilvl + class.
            if def.required_level > item.ilvl {
                return Err(format!(
                    "mod {mod_id} requires ilvl {} but item has ilvl {}",
                    def.required_level, item.ilvl
                ));
            }
            ensure_group_free(registry, &item, def)?;
            // Capacity is checked at the post-transition rarity: the recorded
            // currency may promote the item (a Regal on a full Magic item sees
            // Rare capacity), and an explicit mod on a Normal item implies at
            // least Magic — the engine's transmute path; Normal holds 0
            // explicits.
            let effective_rarity = {
                use Rarity::*;
                let target = currency.as_deref().and_then(currency_target_rarity);
                match (item.rarity, target) {
                    (Normal, Some(r)) => r,
                    (Normal, None) => Magic,
                    (Magic, Some(Rare)) => Rare,
                    (cur, _) => cur,
                }
            };
            ensure_slot_open(&item, def.affix_type, effective_rarity)?;
            let t = roll.unwrap_or(0.5).clamp(0.0, 1.0);
            let values = def.stats.iter().map(|s| s.roll(t)).collect();
            let roll = ModRoll {
                mod_id: mid,
                affix_type: def.affix_type,
                kind: def.kind,
                values,
                is_fractured: false,
            };
            match def.affix_type {
                AffixType::Prefix => item.prefixes.push(roll),
                AffixType::Suffix => item.suffixes.push(roll),
                _ => return Err("only prefix/suffix outcomes supported here".into()),
            }
            // Persist the rarity transition (upgrade-only by construction).
            item.rarity = effective_rarity;
            Ok(RecordOutcomeResponse {
                item,
                change: "added".into(),
                explanation: format!("added {mod_id}"),
            })
        }
        OutcomeKind::RemoveMod { affix, index } => {
            let removed_id = remove_outcome_slot(&mut item, &affix, index)?;
            Ok(RecordOutcomeResponse {
                item,
                change: "removed".into(),
                explanation: format!("removed {removed_id}"),
            })
        }
        OutcomeKind::ReplaceMod {
            remove_affix,
            remove_index,
            add_mod_id,
            roll,
        } => {
            let removed_id = remove_outcome_slot(&mut item, &remove_affix, remove_index)?;
            let mid = ModId::from(add_mod_id.clone());
            let def = registry
                .get(&mid)
                .ok_or_else(|| format!("unknown mod id: {add_mod_id}"))?;
            if def.required_level > item.ilvl {
                return Err(format!(
                    "mod {add_mod_id} requires ilvl {} but item has ilvl {}",
                    def.required_level, item.ilvl
                ));
            }
            ensure_group_free(registry, &item, def)?;
            // The freed slot only helps the same affix side; a cross-side
            // replacement still needs an open slot at the item's rarity
            // (engine Chaos samples over open slots only).
            ensure_slot_open(&item, def.affix_type, item.rarity)?;
            let t = roll.unwrap_or(0.5).clamp(0.0, 1.0);
            let values = def.stats.iter().map(|s| s.roll(t)).collect();
            let new_roll = ModRoll {
                mod_id: mid,
                affix_type: def.affix_type,
                kind: def.kind,
                values,
                is_fractured: false,
            };
            match def.affix_type {
                AffixType::Prefix => item.prefixes.push(new_roll),
                AffixType::Suffix => item.suffixes.push(new_roll),
                _ => return Err("only prefix/suffix replacement supported".into()),
            }
            Ok(RecordOutcomeResponse {
                item,
                change: "replaced".into(),
                explanation: format!("replaced {removed_id} with {add_mod_id}"),
            })
        }
        OutcomeKind::RerollValues { rolls, sanctify } => {
            apply_reroll_values(&mut item, registry, &rolls, sanctify)
        }
        OutcomeKind::SetRarity { rarity } => {
            let r: Rarity = serde_json::from_value(serde_json::json!(rarity))
                .map_err(|e| format!("invalid rarity {rarity}: {e}"))?;
            item.rarity = r;
            Ok(RecordOutcomeResponse {
                item,
                change: "rarity".into(),
                explanation: format!("set rarity to {rarity}"),
            })
        }
    }
}

/// Apply a Divine-Orb-style value reroll to one or more existing mods.
///
/// - `slot` ∈ {`"implicit"`, `"prefix"`, `"suffix"`}; index is slot-local.
/// - Each mod's `mod_id`/`affix_type` is preserved (Divine never changes
///   the tier).
/// - Fractured mods are rejected (the engine's Divine impl skips them
///   silently; here we surface the error so the dialog can warn).
/// - When `sanctify == false`: each value must lie in `[def.stats[i].min,
///   def.stats[i].max]`.
/// - When `sanctify == true`: requires Rare rarity; values may lie in
///   `[def.stats[i].min × 0.8, def.stats[i].max × 1.2]` (Omen of
///   Sanctification mechanics) and `Item.sanctified` is set.
/// - Corrupted items are rejected (engine semantics).
fn apply_reroll_values(
    item: &mut Item,
    registry: &ModRegistry,
    rolls: &[RerolledMod],
    sanctify: bool,
) -> Result<RecordOutcomeResponse, String> {
    if item.corrupted {
        return Err("Divine Orb cannot be applied to a corrupted item".into());
    }
    if sanctify && item.rarity != Rarity::Rare {
        return Err("Omen of Sanctification requires a Rare item".into());
    }

    let mut updated: usize = 0;
    let mut by_slot: std::collections::HashMap<&str, Vec<&RerolledMod>> =
        std::collections::HashMap::new();
    for r in rolls {
        by_slot.entry(r.slot.as_str()).or_default().push(r);
    }

    for (slot, entries) in &by_slot {
        // Pre-validate each entry against the chosen slot before mutating.
        let target_len = match *slot {
            "implicit" => item.implicits.len(),
            "prefix" => item.prefixes.len(),
            "suffix" => item.suffixes.len(),
            other => return Err(format!("invalid slot: {other}")),
        };
        for r in entries {
            if r.index >= target_len {
                return Err(format!("{slot} index {} out of range", r.index));
            }
        }
    }

    for (slot, entries) in by_slot {
        for r in entries {
            let target: &mut ModRoll = match slot {
                "implicit" => &mut item.implicits[r.index],
                "prefix" => &mut item.prefixes[r.index],
                "suffix" => &mut item.suffixes[r.index],
                _ => unreachable!("validated above"),
            };
            if target.is_fractured {
                return Err(format!(
                    "{slot} {} is fractured; Divine cannot reroll it",
                    r.index
                ));
            }
            let def = registry
                .get(&target.mod_id)
                .ok_or_else(|| format!("unknown mod id: {}", target.mod_id.as_str()))?;
            if r.values.len() != def.stats.len() {
                return Err(format!(
                    "{slot} {} expects {} stat values, got {}",
                    r.index,
                    def.stats.len(),
                    r.values.len()
                ));
            }
            for (i, v) in r.values.iter().enumerate() {
                let stat = &def.stats[i];
                let (lo, hi) = if sanctify {
                    (stat.min * 0.8, stat.max * 1.2)
                } else {
                    (stat.min, stat.max)
                };
                if !v.is_finite() || *v < lo || *v > hi {
                    return Err(format!(
                        "{slot} {} stat {} value {v} outside allowed range [{lo:.4}, {hi:.4}]",
                        r.index, i,
                    ));
                }
            }
            target.values = r.values.iter().copied().collect();
            updated += 1;
        }
    }

    if sanctify {
        item.sanctified = true;
    }

    Ok(RecordOutcomeResponse {
        item: item.clone(),
        change: if sanctify {
            "sanctified".into()
        } else {
            "rerolled".into()
        },
        explanation: if sanctify {
            format!(
                "sanctified {updated} mod{}; values rerolled within widened bounds and item locked",
                if updated == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "rerolled values on {updated} mod{}",
                if updated == 1 { "" } else { "s" }
            )
        },
    })
}

/// Rarity a mod-adding currency promotes the item to (Transmute-class →
/// Magic; Regal/Exalt/Chaos-class → Rare). Currencies never downgrade.
fn currency_target_rarity(currency: &str) -> Option<Rarity> {
    match currency {
        "OrbOfTransmutation" | "GreaterOrbOfTransmutation" | "PerfectOrbOfTransmutation" => {
            Some(Rarity::Magic)
        }
        "RegalOrb" | "GreaterRegalOrb" | "PerfectRegalOrb" | "ExaltedOrb" | "GreaterExaltedOrb"
        | "PerfectExaltedOrb" | "ChaosOrb" | "GreaterChaosOrb" | "PerfectChaosOrb" => {
            Some(Rarity::Rare)
        }
        _ => None,
    }
}

/// Mod-group exclusivity: at most one mod per group on the item.
fn ensure_group_free(
    registry: &ModRegistry,
    item: &Item,
    def: &ModDefinition,
) -> Result<(), String> {
    for m in item.prefixes.iter().chain(item.suffixes.iter()) {
        if let Some(g) = registry.group_of(&m.mod_id) {
            if g.as_str() == def.mod_group.0.as_str() {
                return Err(format!(
                    "mod-group {} already occupied by {}",
                    def.mod_group.0.as_str(),
                    m.mod_id
                ));
            }
        }
    }
    Ok(())
}

/// Open-slot check at `rarity`. Capacities come from the engine's
/// [`Rarity::max_prefixes`]/[`Rarity::max_suffixes`] with class max 3,
/// matching the engine's basic-currency apply path: Normal 0/0, Magic 1/1,
/// Rare 3/3.
fn ensure_slot_open(item: &Item, affix: AffixType, rarity: Rarity) -> Result<(), String> {
    match affix {
        AffixType::Prefix if item.prefixes.len() >= usize::from(rarity.max_prefixes(3)) => {
            Err("no open prefix slots".into())
        }
        AffixType::Suffix if item.suffixes.len() >= usize::from(rarity.max_suffixes(3)) => {
            Err("no open suffix slots".into())
        }
        _ => Ok(()),
    }
}

fn remove_outcome_slot(item: &mut Item, affix: &str, index: usize) -> Result<String, String> {
    let af: AffixType = match affix {
        "prefix" => AffixType::Prefix,
        "suffix" => AffixType::Suffix,
        other => return Err(format!("invalid affix: {other}")),
    };
    let removed = match af {
        AffixType::Prefix => {
            if index >= item.prefixes.len() {
                return Err("prefix index out of range".into());
            }
            // Fractured rolls are immutable — Annul/Chaos cannot remove them.
            if item.prefixes[index].is_fractured {
                return Err(format!("prefix {index} is fractured; it cannot be removed"));
            }
            item.prefixes.remove(index)
        }
        AffixType::Suffix => {
            if index >= item.suffixes.len() {
                return Err("suffix index out of range".into());
            }
            if item.suffixes[index].is_fractured {
                return Err(format!("suffix {index} is fractured; it cannot be removed"));
            }
            item.suffixes.remove(index)
        }
        _ => return Err("only prefix/suffix removal supported".into()),
    };
    Ok(removed.mod_id.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poc2_engine::ids::{BaseTypeId, ConceptId, ItemClassId, ModGroupId, StatId, TagId};
    use poc2_engine::item::QualityKind;
    use poc2_engine::mods::{ModDomain, ModFlags, ModGroup, ModKind, ModStat, SpawnWeight};
    use poc2_engine::patch::PatchRange;
    use smallvec::smallvec;

    fn mk_mod(id: &str, group: &str, affix: AffixType) -> ModDefinition {
        ModDefinition {
            id: ModId::from(id),
            name: Some(id.to_string()),
            mod_group: ModGroup(ModGroupId::from(group)),
            affix_type: affix,
            kind: ModKind::Explicit,
            domain: ModDomain::Item,
            tags: smallvec![],
            concept_set: smallvec![ConceptId::from("EnergyShield")],
            spawn_weights: smallvec![SpawnWeight {
                tag: TagId::from("default"),
                weight: 100
            }],
            stats: smallvec![ModStat {
                stat_id: StatId::from("s"),
                min: 10.0,
                max: 20.0
            }],
            required_level: 1,
            tier: Some(1),
            allowed_item_classes: smallvec![ItemClassId::from("BodyArmour")],
            patch_range: PatchRange::ALL,
            flags: ModFlags::empty(),
            text_template: None,
        }
    }

    fn registry() -> ModRegistry {
        ModRegistry::from_mods(
            vec![
                mk_mod("EsP1", "g-es", AffixType::Prefix),
                mk_mod("EsP2", "g-es", AffixType::Prefix),
                mk_mod("ArmourP1", "g-armour", AffixType::Prefix),
                mk_mod("LifeP1", "g-life", AffixType::Prefix),
                mk_mod("SpiritP1", "g-spirit", AffixType::Prefix),
                mk_mod("FireResS1", "g-fire", AffixType::Suffix),
                mk_mod("ColdResS1", "g-cold", AffixType::Suffix),
                mk_mod("LightResS1", "g-light", AffixType::Suffix),
                mk_mod("ChaosResS1", "g-chaos", AffixType::Suffix),
            ],
            vec![],
        )
    }

    fn roll_of(id: &str, affix: AffixType) -> ModRoll {
        ModRoll::new(ModId::from(id), affix, ModKind::Explicit)
    }

    fn item_with(rarity: Rarity, prefixes: &[&str], suffixes: &[&str]) -> Item {
        Item {
            base: BaseTypeId::from("BodyArmour"),
            ilvl: 80,
            rarity,
            corrupted: false,
            sanctified: false,
            mirrored: false,
            quality: 0,
            quality_kind: QualityKind::Untagged,
            implicits: smallvec![],
            prefixes: prefixes
                .iter()
                .map(|id| roll_of(id, AffixType::Prefix))
                .collect(),
            suffixes: suffixes
                .iter()
                .map(|id| roll_of(id, AffixType::Suffix))
                .collect(),
            enchantments: smallvec![],
            hidden_desecrated: None,
            sockets: smallvec![],
            hinekora_lock: None,
        }
    }

    fn add_args(item: Item, mod_id: &str, currency: Option<&str>) -> RecordOutcomeArgs {
        RecordOutcomeArgs {
            item,
            outcome: OutcomeKind::AddMod {
                mod_id: mod_id.into(),
                roll: None,
                currency: currency.map(str::to_string),
            },
        }
    }

    #[test]
    fn magic_item_has_one_prefix_slot() {
        // Under the old 3/3 assumption this add would have succeeded.
        let item = item_with(Rarity::Magic, &["EsP1"], &[]);
        let err = record_outcome(&registry(), add_args(item, "ArmourP1", None)).unwrap_err();
        assert!(err.contains("no open prefix slots"), "got: {err}");
    }

    #[test]
    fn magic_item_open_suffix_still_accepts() {
        let item = item_with(Rarity::Magic, &["EsP1"], &[]);
        let resp = record_outcome(&registry(), add_args(item, "FireResS1", None)).unwrap();
        assert_eq!(resp.item.suffixes.len(), 1);
        assert_eq!(resp.item.rarity, Rarity::Magic);
    }

    #[test]
    fn regal_on_full_magic_uses_rare_capacity() {
        let item = item_with(Rarity::Magic, &["EsP1"], &["FireResS1"]);
        let resp =
            record_outcome(&registry(), add_args(item, "ArmourP1", Some("RegalOrb"))).unwrap();
        assert_eq!(resp.item.prefixes.len(), 2);
        assert_eq!(resp.item.rarity, Rarity::Rare);
    }

    #[test]
    fn normal_add_implies_magic() {
        let item = item_with(Rarity::Normal, &[], &[]);
        let resp = record_outcome(&registry(), add_args(item, "EsP1", None)).unwrap();
        assert_eq!(resp.item.rarity, Rarity::Magic);

        let err = record_outcome(&registry(), add_args(resp.item, "ArmourP1", None)).unwrap_err();
        assert!(err.contains("no open prefix slots"), "got: {err}");
    }

    #[test]
    fn rare_prefixes_cap_at_three() {
        let item = item_with(Rarity::Rare, &["EsP1", "ArmourP1", "LifeP1"], &[]);
        let err = record_outcome(&registry(), add_args(item, "SpiritP1", None)).unwrap_err();
        assert!(err.contains("no open prefix slots"), "got: {err}");
    }

    #[test]
    fn fractured_mod_cannot_be_removed() {
        let mut item = item_with(Rarity::Rare, &["EsP1"], &[]);
        item.prefixes[0].is_fractured = true;
        let err = record_outcome(
            &registry(),
            RecordOutcomeArgs {
                item,
                outcome: OutcomeKind::RemoveMod {
                    affix: "prefix".into(),
                    index: 0,
                },
            },
        )
        .unwrap_err();
        assert!(err.contains("fractured"), "got: {err}");
    }

    #[test]
    fn replace_cross_affix_requires_open_slot() {
        let item = item_with(
            Rarity::Rare,
            &["EsP1"],
            &["FireResS1", "ColdResS1", "LightResS1"],
        );
        let err = record_outcome(
            &registry(),
            RecordOutcomeArgs {
                item,
                outcome: OutcomeKind::ReplaceMod {
                    remove_affix: "prefix".into(),
                    remove_index: 0,
                    add_mod_id: "ChaosResS1".into(),
                    roll: None,
                },
            },
        )
        .unwrap_err();
        assert!(err.contains("no open suffix slots"), "got: {err}");
    }

    #[test]
    fn replace_respects_group_exclusivity() {
        let item = item_with(Rarity::Rare, &["EsP1", "ArmourP1"], &[]);
        let err = record_outcome(
            &registry(),
            RecordOutcomeArgs {
                item,
                outcome: OutcomeKind::ReplaceMod {
                    remove_affix: "prefix".into(),
                    remove_index: 1,
                    add_mod_id: "EsP2".into(),
                    roll: None,
                },
            },
        )
        .unwrap_err();
        assert!(err.contains("mod-group g-es"), "got: {err}");
    }
}
