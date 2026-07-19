//! `reroll` command — enumerate the mods a Divine-style reroll would touch.
//!
//! Ported verbatim from the Tauri desktop `rerollable_mods` command. Pure
//! compute over the [`ModRegistry`] and an [`Item`]; the only external inputs
//! are the item, the active omen id (if any), and the patch for the response.
//! No Tauri / disk / network concerns survive the port.

use poc2_engine::item::Item;
use poc2_engine::patch::PatchVersion;
use poc2_engine::ModRegistry;
use serde::Serialize;

/// One stat band on a rerollable mod, with both the sanctified-widened bounds
/// (`min`/`max`) and the strict bounds (`strict_min`/`strict_max`) so the UI
/// can label the widened band even when sanctification is active.
#[derive(Debug, Serialize)]
pub struct RerollableStatView {
    pub stat_id: String,
    /// Lower bound the player can record. For sanctification this is
    /// `def.min × 0.8`; otherwise `def.min`.
    pub min: f64,
    /// Upper bound the player can record. For sanctification this is
    /// `def.max × 1.2`; otherwise `def.max`.
    pub max: f64,
    /// Strict (non-sanctified) lower bound. Surfaced so the UI can label
    /// the widened band even when sanctification is active.
    pub strict_min: f64,
    /// Strict (non-sanctified) upper bound.
    pub strict_max: f64,
    /// Currently rolled value for this stat.
    pub current: f64,
}

/// A single mod that a Divine-style reroll would affect, with its tier ladder
/// position and the per-stat bands.
#[derive(Debug, Serialize)]
pub struct RerollableMod {
    /// `"implicit"`, `"prefix"`, or `"suffix"`.
    pub slot: String,
    /// Slot-local index.
    pub index: usize,
    pub mod_id: String,
    pub name: Option<String>,
    pub text_template: Option<String>,
    /// Tier number within the mod-group ladder (1 = highest).
    pub tier_index: u32,
    /// Total tiers in the ladder.
    pub tier_count: u32,
    /// Fractured mods are skipped by Divine; the UI greys them out.
    pub is_fractured: bool,
    pub stats: Vec<RerollableStatView>,
}

/// Response of the `reroll` command: the omen-derived flags plus the list of
/// mods a reroll would touch.
#[derive(Debug, Serialize)]
pub struct RerollableModsResponse {
    /// Patch the registry was loaded for.
    pub patch: String,
    /// Whether the active omen widens value bounds (Sanctification).
    pub sanctify: bool,
    /// Whether the active omen restricts Divine to implicits (Blessed).
    pub implicits_only: bool,
    pub mods: Vec<RerollableMod>,
}

/// Enumerate the mods a Divine-style reroll would affect on `item`.
///
/// `omen` is the active omen id, if any. Recognised values:
/// - `"OmenOfTheBlessed"` → only implicits returned.
/// - `"OmenOfSanctification"` → widened sanctified bounds in stats.
/// - other / `None` → plain Divine.
pub fn rerollable_mods(
    registry: &ModRegistry,
    item: &Item,
    omen: Option<&str>,
    patch: PatchVersion,
) -> RerollableModsResponse {
    let sanctify = matches!(omen, Some("OmenOfSanctification"));
    let implicits_only = matches!(omen, Some("OmenOfTheBlessed"));

    let mut out: Vec<RerollableMod> = Vec::new();

    let push = |out: &mut Vec<RerollableMod>,
                slot: &str,
                index: usize,
                roll: &poc2_engine::item::ModRoll| {
        let def = match registry.get(&roll.mod_id) {
            Some(d) => d,
            None => return, // unknown mod (legacy data) — skip silently
        };
        // Tier ladder for this mod-group: order by descending required_level.
        let group_members = registry.group_members(&def.mod_group.0);
        let mut levels: Vec<u32> = group_members
            .iter()
            .filter_map(|i| registry.at(*i).map(|m| m.required_level))
            .collect();
        levels.sort_unstable_by(|a, b| b.cmp(a));
        levels.dedup();
        let tier_count = levels.len().max(1) as u32;
        let tier_index = levels
            .iter()
            .position(|l| *l == def.required_level)
            .map(|p| (p + 1) as u32)
            .unwrap_or(1);

        let stats: Vec<RerollableStatView> = def
            .stats
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let current = roll.values.get(i).copied().unwrap_or(s.min);
                let (lo, hi) = if sanctify {
                    (s.min * 0.8, s.max * 1.2)
                } else {
                    (s.min, s.max)
                };
                RerollableStatView {
                    stat_id: s.stat_id.as_str().to_string(),
                    min: lo,
                    max: hi,
                    strict_min: s.min,
                    strict_max: s.max,
                    current,
                }
            })
            .collect();

        out.push(RerollableMod {
            slot: slot.to_string(),
            index,
            mod_id: roll.mod_id.as_str().to_string(),
            name: def.name.clone(),
            text_template: def.text_template.clone(),
            tier_index,
            tier_count,
            is_fractured: roll.is_fractured,
            stats,
        });
    };

    for (i, roll) in item.implicits.iter().enumerate() {
        push(&mut out, "implicit", i, roll);
    }
    if !implicits_only {
        for (i, roll) in item.prefixes.iter().enumerate() {
            push(&mut out, "prefix", i, roll);
        }
        for (i, roll) in item.suffixes.iter().enumerate() {
            push(&mut out, "suffix", i, roll);
        }
    }

    RerollableModsResponse {
        patch: format!("{patch}"),
        sanctify,
        implicits_only,
        mods: out,
    }
}
