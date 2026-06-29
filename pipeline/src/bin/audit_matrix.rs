//! `audit-matrix` — full crafting-surface audit of the engine + advisor
//! against a real data bundle.
//!
//! Sweeps every item class that carries an explicit mod pool × base tiers
//! (lowest / median / highest drop-level released base) × item levels ×
//! every currency the resolver knows (basic orbs, Greater/Perfect variants,
//! Vaal, bones, essences, catalysts, alloys, emotions, fracturing,
//! Hinekora's Lock), plus advisor `plan()` legality per class and rarity.
//!
//! Read-only: emits a JSON report + console summary. Exit code is 1 only
//! when the harness itself cannot run (e.g. bundle missing) — findings are
//! reported, not fatal, so the report stays inspectable either way.
//!
//! ```bash
//! cargo run --release -p poc2-pipeline --bin audit-matrix -- \
//!   --bundle ~/.config/poc2/bundles/poc2.bundle.json.gz \
//!   --out /tmp/poc2_audit/matrix_report.json
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Parser;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use serde::Serialize;
use smallvec::smallvec;

use poc2_advisor::stash::Stash;
use poc2_advisor::AdvisorAction;
use poc2_advisor::{plan, BeamConfig, Goal, PlanInput};
use poc2_engine::currency::{reveal_at_well_of_souls, sample_reveal_options};
use poc2_engine::mods::{ModFlags, ModKind};
use poc2_engine::{
    apply_currency_with_bases, AffixType, BaseRegistry, Currency, CurrencyId, CurrencyResolver,
    DefaultCurrencyResolver, Item, ItemClassId, League, ModRegistry, OmenSet, PatchVersion,
    QualityKind, Rarity, ReleaseState,
};
use poc2_market::Valuator;
use poc2_rules::RuleSet;
use poc2_strategies::dsl::{Target, TargetSpec};
use poc2_strategies::{seed_strategies, StrategyRegistry};

#[derive(Parser, Debug)]
#[command(about = "Audit the crafting engine + advisor across every class/tier/currency")]
struct Args {
    /// Bundle path. Defaults to ~/.config/poc2/bundles/poc2.bundle.json.gz
    #[arg(long)]
    bundle: Option<PathBuf>,
    /// JSON report output path.
    #[arg(long, default_value = "/tmp/poc2_audit/matrix_report.json")]
    out: PathBuf,
    /// RNG seed for the deterministic sweep.
    #[arg(long, default_value_t = 0xC0FFEE)]
    seed: u64,
    /// Dump each class's effective mod pools (weight-resolved at ilvl 82 on
    /// the top base) to this path as JSON, then exit. Used to diff against
    /// poe2db ModsView extracts.
    #[arg(long)]
    dump_pools: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Pass,
    Fail,
    Warn,
    Info,
}

#[derive(Debug, Serialize)]
struct Check {
    class: String,
    base: String,
    ilvl: u32,
    check: String,
    status: Status,
    detail: String,
}

#[derive(Serialize)]
struct Report {
    bundle_patch: String,
    classes_audited: usize,
    checks: Vec<Check>,
}

struct Ctx {
    registry: ModRegistry,
    base_registry: BaseRegistry,
    resolver: DefaultCurrencyResolver,
    rules: RuleSet,
    strategies: StrategyRegistry,
    valuator: Valuator,
    patch: PatchVersion,
    league: League,
    /// Every currency id the resolver can resolve, for gating sweeps.
    all_currency_ids: Vec<CurrencyId>,
    /// Essence ids paired with their target mod + quality, for class sweeps.
    essences: Vec<poc2_engine::Essence>,
    catalyst_ids: Vec<CurrencyId>,
    checks: Vec<Check>,
}

const ILVLS: [u32; 8] = [1, 20, 35, 44, 50, 55, 70, 82];

const BASIC_ORBS: [&str; 21] = [
    "OrbOfTransmutation",
    "GreaterOrbOfTransmutation",
    "PerfectOrbOfTransmutation",
    "OrbOfAugmentation",
    "GreaterOrbOfAugmentation",
    "PerfectOrbOfAugmentation",
    "RegalOrb",
    "GreaterRegalOrb",
    "PerfectRegalOrb",
    "OrbOfAlchemy",
    "ExaltedOrb",
    "GreaterExaltedOrb",
    "PerfectExaltedOrb",
    "OrbOfAnnulment",
    "ChaosOrb",
    "GreaterChaosOrb",
    "PerfectChaosOrb",
    "DivineOrb",
    "VaalOrb",
    "HinekorasLock",
    "FracturingOrb",
];

// The real 0.5 bone catalogue (poe2db Desecrated_Modifiers): Cranium is
// Preserved-only; Altered exists only as Collarbone (otherworldly reveals).
const BONES: [&str; 11] = [
    "GnawedRib",
    "GnawedJawbone",
    "GnawedCollarbone",
    "PreservedRib",
    "PreservedJawbone",
    "PreservedCollarbone",
    "PreservedCranium",
    "AncientRib",
    "AncientJawbone",
    "AncientCollarbone",
    "AlteredCollarbone",
];

fn main() {
    let args = Args::parse();
    let bundle_path = args.bundle.clone().unwrap_or_else(|| {
        let home = std::env::var("HOME").expect("HOME not set and --bundle not given");
        PathBuf::from(home).join(".config/poc2/bundles/poc2.bundle.json.gz")
    });
    let bundle = match poc2_data::io::read_bundle(&bundle_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "FATAL: bundle at {} failed to load: {e}",
                bundle_path.display()
            );
            std::process::exit(1);
        }
    };

    // Mirror crates/poc2-wasm EngineState::from_bundle exactly.
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let strategies = StrategyRegistry::from_strategies(seed_strategies());
    let rules = RuleSet::from_rules(poc2_rules::seed_rules());
    let essences = bundle.essence_catalogue();
    let catalysts = bundle.catalyst_catalogue();
    let mut alloy_likes = bundle.alloy_catalogue();
    alloy_likes.extend(bundle.emotion_catalogue());
    let resolver = DefaultCurrencyResolver::new()
        .with_essences(bundle.essence_catalogue())
        .with_catalysts(bundle.catalyst_catalogue())
        .with_alloys(alloy_likes);

    let mut all_currency_ids: Vec<CurrencyId> = BASIC_ORBS
        .iter()
        .chain(BONES.iter())
        .map(|s| CurrencyId::from(*s))
        .collect();
    // Bundle catalogue (12 CoE base catalysts) + engine presets (24 incl.
    // the jewel-gated Refined variants) — dedupe below keeps one of each.
    let mut catalyst_ids: Vec<CurrencyId> = catalysts.iter().map(|c| c.id().clone()).collect();
    catalyst_ids.extend(
        poc2_engine::Catalyst::default_catalogue()
            .iter()
            .map(|c| c.id().clone()),
    );
    {
        let mut seen = BTreeSet::new();
        catalyst_ids.retain(|c| seen.insert(c.as_str().to_string()));
    }
    all_currency_ids.extend(catalyst_ids.iter().cloned());
    all_currency_ids.extend(essences.iter().map(|e| e.id.clone()));
    all_currency_ids.extend(resolver.alloys().iter().map(|a| a.id.clone()));
    // Dedupe (catalyst presets may shadow catalogue entries).
    let mut seen = BTreeSet::new();
    all_currency_ids.retain(|c| seen.insert(c.as_str().to_string()));

    let mut ctx = Ctx {
        registry,
        base_registry,
        resolver,
        rules,
        strategies,
        valuator: Valuator::default(),
        patch: bundle.header.game_patch,
        league: League::current(),
        all_currency_ids,
        essences,
        catalyst_ids,
        checks: Vec::new(),
    };

    // Classes with an explicit craftable pool, from the bundle itself.
    let mut class_names: BTreeSet<String> = BTreeSet::new();
    for m in &bundle.mods {
        if m.kind == ModKind::Explicit {
            for c in &m.allowed_item_classes {
                class_names.insert(c.as_str().to_string());
            }
        }
    }
    let classes: Vec<ItemClassId> = class_names
        .iter()
        .map(|s| ItemClassId::from(s.as_str()))
        .collect();

    // Per class: released bases at min / median / max drop level.
    let mut bases_by_class: BTreeMap<String, Vec<(String, String, u32)>> = BTreeMap::new();
    for b in &bundle.base_items {
        if b.release_state != ReleaseState::Released {
            continue;
        }
        bases_by_class
            .entry(b.item_class.as_str().to_string())
            .or_default()
            .push((b.id.as_str().to_string(), b.name.clone(), b.drop_level));
    }
    for v in bases_by_class.values_mut() {
        v.sort_by_key(|(_, _, dl)| *dl);
    }

    if let Some(path) = &args.dump_pools {
        dump_pools(&ctx, &classes, &bases_by_class, path);
        return;
    }

    let n_classes = classes.len();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(args.seed);

    for class in &classes {
        let Some(bases) = bases_by_class.get(class.as_str()) else {
            ctx.push(
                class.as_str(),
                "-",
                0,
                "bases",
                Status::Warn,
                "class has mods but no released bases in bundle",
            );
            continue;
        };
        // Lowest, median, highest drop-level base — the class's base tiers.
        let mut picks: Vec<(String, String, u32)> = vec![bases[0].clone()];
        if bases.len() > 2 {
            picks.push(bases[bases.len() / 2].clone());
        }
        if bases.len() > 1 {
            picks.push(bases[bases.len() - 1].clone());
        }
        picks.dedup_by(|a, b| a.0 == b.0);

        for (base_id, base_name, drop_level) in &picks {
            for ilvl in ILVLS {
                if ilvl < *drop_level {
                    continue; // base cannot exist at this ilvl
                }
                ctx.pool_census(class, base_id, base_name, ilvl);
                ctx.ladder(class, base_id, base_name, ilvl, &mut rng);
            }
        }

        // Deep checks on the top base at ilvl 82 only.
        let (top_base, top_name, _) = picks.last().expect("picks non-empty").clone();
        ctx.alchemy(class, &top_base, &top_name, &mut rng);
        ctx.variant_floors(class, &top_base, &top_name, &mut rng);
        ctx.corruption_lockout(class, &top_base, &top_name, &mut rng);
        ctx.unique_gating(class, &top_base, &top_name, &mut rng);
        ctx.fracturing(class, &top_base, &top_name, &mut rng);
        ctx.bones(class, &top_base, &top_name, &mut rng);
        ctx.essence_sweep(class, &top_base, &top_name, &mut rng);
        ctx.catalyst_sweep(class, &top_base, &top_name, &mut rng);
        ctx.alloy_sweep(class, &top_base, &top_name, &mut rng);
        ctx.advisor(class, &top_base, &top_name, &mut rng);
    }

    // ---- report ----
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &ctx.checks {
        *by_status
            .entry(match c.status {
                Status::Pass => "pass",
                Status::Fail => "fail",
                Status::Warn => "warn",
                Status::Info => "info",
            })
            .or_default() += 1;
    }
    println!("\n=== audit-matrix summary ===");
    println!("classes audited: {n_classes}");
    for (k, v) in &by_status {
        println!("  {k}: {v}");
    }
    let mut fail_by_check: BTreeMap<String, usize> = BTreeMap::new();
    for c in ctx.checks.iter().filter(|c| c.status == Status::Fail) {
        *fail_by_check.entry(c.check.clone()).or_default() += 1;
    }
    if !fail_by_check.is_empty() {
        println!("\nfailures by check:");
        for (k, v) in &fail_by_check {
            println!("  {k}: {v}");
        }
        println!("\nfirst 60 failures:");
        for c in ctx
            .checks
            .iter()
            .filter(|c| c.status == Status::Fail)
            .take(60)
        {
            println!(
                "  [{}] {} base={} ilvl={} — {}",
                c.check, c.class, c.base, c.ilvl, c.detail
            );
        }
    }
    let report = Report {
        bundle_patch: ctx.patch.to_string(),
        classes_audited: n_classes,
        checks: ctx.checks,
    };
    if let Some(dir) = args.out.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(
        &args.out,
        serde_json::to_vec_pretty(&report).expect("serialize report"),
    )
    .expect("write report");
    println!("\nreport: {}", args.out.display());
}

/// Serialize each class's effective pools at ilvl 82 on its highest
/// drop-level released base: every explicit mod allowed on the class,
/// annotated with its resolved inclusive weight (0 = unrollable on the
/// base), plus the desecrated / corrupted / essence-only / otherworldly
/// category sets. The poe2db diff tooling consumes this.
fn dump_pools(
    ctx: &Ctx,
    classes: &[ItemClassId],
    bases_by_class: &BTreeMap<String, Vec<(String, String, u32)>>,
    path: &std::path::Path,
) {
    use serde_json::json;
    let mut out = serde_json::Map::new();
    for class in classes {
        let Some(bases) = bases_by_class.get(class.as_str()) else {
            continue;
        };
        let Some((base_id, base_name, _)) = bases.last() else {
            continue;
        };
        let base_tid = poc2_engine::ids::BaseTypeId::from(base_id.as_str());
        let tags = ctx.base_registry.tags_of(&base_tid).to_vec();
        let mut mods = Vec::new();
        for affix in [AffixType::Prefix, AffixType::Suffix] {
            for &idx in ctx.registry.for_class_affix(class, affix) {
                let Some(m) = ctx.registry.at(idx) else {
                    continue;
                };
                if !m.patch_range.contains(ctx.patch) {
                    continue;
                }
                let w = ctx
                    .registry
                    .inclusive_weight_for_on_base(m, &base_tid, 82, class, &tags);
                let category = if m.kind == ModKind::Desecrated {
                    if m.flags.contains(ModFlags::OTHERWORLDLY) {
                        "otherworldly"
                    } else {
                        "desecrated"
                    }
                } else if m.kind == poc2_engine::mods::ModKind::Corrupted {
                    "corrupted"
                } else if m.flags.contains(ModFlags::ESSENCE_ONLY) {
                    "essence_only"
                } else if m.kind == poc2_engine::mods::ModKind::Crafted {
                    "crafted"
                } else {
                    "base"
                };
                mods.push(json!({
                    "id": m.id.as_str(),
                    "name": m.name,
                    "group": m.mod_group.0.as_str(),
                    "affix": if affix == AffixType::Prefix { "prefix" } else { "suffix" },
                    "kind": format!("{:?}", m.kind),
                    "category": category,
                    "req": m.required_level,
                    "tier": m.tier,
                    "weight": w,
                    "stats": m.stats.iter().map(|st| json!({
                        "stat": st.stat_id.as_str(), "min": st.min, "max": st.max
                    })).collect::<Vec<_>>(),
                }));
            }
        }
        out.insert(
            class.as_str().to_string(),
            json!({ "base": base_id, "base_name": base_name, "mods": mods }),
        );
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&serde_json::Value::Object(out)).expect("serialize pools"),
    )
    .expect("write pools");
    println!("pools dumped: {}", path.display());
}

impl Ctx {
    fn push(
        &mut self,
        class: &str,
        base: &str,
        ilvl: u32,
        check: &str,
        status: Status,
        detail: impl Into<String>,
    ) {
        self.checks.push(Check {
            class: class.to_string(),
            base: base.to_string(),
            ilvl,
            check: check.to_string(),
            status,
            detail: detail.into(),
        });
    }

    fn mk_item(&self, base_id: &str, ilvl: u32, rarity: Rarity) -> Item {
        Item {
            base: base_id.into(),
            ilvl,
            rarity,
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

    fn apply_id(
        &self,
        id: &str,
        item: &mut Item,
        rng: &mut Xoshiro256PlusPlus,
    ) -> Result<(), String> {
        let cid = CurrencyId::from(id);
        let Some(c) = self.resolver.resolve(&cid) else {
            return Err(format!("resolver cannot resolve {id}"));
        };
        let mut omens = OmenSet::new();
        apply_currency_with_bases(
            c.as_ref(),
            item,
            &self.registry,
            &self.base_registry,
            rng,
            self.patch,
            &mut omens,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Item-state invariants that must hold after ANY successful apply.
    fn invariants(&mut self, class: &ItemClassId, base: &str, ilvl: u32, stage: &str, item: &Item) {
        let max_side = match item.rarity {
            Rarity::Normal => 0,
            Rarity::Magic => 1,
            Rarity::Rare | Rarity::Unique => 3,
        };
        if item.prefixes.len() > max_side || item.suffixes.len() > max_side {
            self.push(
                class.as_str(),
                base,
                ilvl,
                "invariant_affix_caps",
                Status::Fail,
                format!(
                    "{stage}: {}p/{}s exceeds cap {max_side} for {:?}",
                    item.prefixes.len(),
                    item.suffixes.len(),
                    item.rarity
                ),
            );
        }
        // mod-group exclusivity + ilvl ceiling + class targeting.
        // Two-phase: collect findings first (registry borrows), push after.
        let mut findings: Vec<(&'static str, String)> = Vec::new();
        let mut groups = BTreeSet::new();
        for roll in item.prefixes.iter().chain(item.suffixes.iter()) {
            let Some(def) = self.registry.get(&roll.mod_id) else {
                findings.push((
                    "invariant_known_mod",
                    format!(
                        "{stage}: rolled mod {} not in registry",
                        roll.mod_id.as_str()
                    ),
                ));
                continue;
            };
            if def.kind == ModKind::Explicit && !groups.insert(def.mod_group.0.as_str().to_string())
            {
                findings.push((
                    "invariant_group_unique",
                    format!("{stage}: duplicate mod group {}", def.mod_group.0.as_str()),
                ));
            }
            if def.kind == ModKind::Explicit && def.required_level > ilvl {
                findings.push((
                    "invariant_ilvl_gate",
                    format!(
                        "{stage}: mod {} req_lvl {} > ilvl {ilvl}",
                        def.id.as_str(),
                        def.required_level
                    ),
                ));
            }
            if def.kind == ModKind::Explicit
                && !def.allowed_item_classes.is_empty()
                && !def.allowed_item_classes.contains(class)
            {
                findings.push((
                    "invariant_class_target",
                    format!(
                        "{stage}: mod {} not allowed on class {}",
                        def.id.as_str(),
                        class.as_str()
                    ),
                ));
            }
            // Basic-orb paths must never produce special-pool mods.
            if stage.starts_with("ladder")
                && def.flags.intersects(
                    ModFlags::ESSENCE_ONLY | ModFlags::DESECRATED_ONLY | ModFlags::CORRUPTED_ONLY,
                )
                && roll.kind == ModKind::Explicit
            {
                findings.push((
                    "invariant_pool_leak",
                    format!(
                        "{stage}: special-pool mod {} (flags {:?}) rolled by basic orb",
                        def.id.as_str(),
                        def.flags
                    ),
                ));
            }
        }
        for (check, detail) in findings {
            self.push(class.as_str(), base, ilvl, check, Status::Fail, detail);
        }
    }

    /// Census of the class's craftable pool at this ilvl.
    fn pool_census(&mut self, class: &ItemClassId, base: &str, _base_name: &str, ilvl: u32) {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for affix in [AffixType::Prefix, AffixType::Suffix] {
            let key = if affix == AffixType::Prefix {
                "prefix"
            } else {
                "suffix"
            };
            for &idx in self.registry.for_class_affix(class, affix) {
                let Some(m) = self.registry.at(idx) else {
                    continue;
                };
                if m.kind != ModKind::Explicit
                    || m.required_level > ilvl
                    || !m.patch_range.contains(self.patch)
                {
                    continue;
                }
                if m.flags.intersects(
                    ModFlags::ESSENCE_ONLY | ModFlags::DESECRATED_ONLY | ModFlags::CORRUPTED_ONLY,
                ) {
                    continue;
                }
                *counts.entry(key).or_default() += 1;
            }
        }
        let p = counts.get("prefix").copied().unwrap_or(0);
        let s = counts.get("suffix").copied().unwrap_or(0);
        if ilvl >= 70 && (p == 0 || s == 0) {
            self.push(
                class.as_str(),
                base,
                ilvl,
                "pool_census",
                Status::Fail,
                format!("empty craftable pool at high ilvl: {p} prefixes / {s} suffixes"),
            );
        } else if p == 0 && s == 0 {
            self.push(
                class.as_str(),
                base,
                ilvl,
                "pool_census",
                Status::Warn,
                format!("empty craftable pool: {p}p/{s}s"),
            );
        } else {
            self.push(
                class.as_str(),
                base,
                ilvl,
                "pool_census",
                Status::Pass,
                format!("{p}p/{s}s"),
            );
        }
    }

    /// Normal → Magic → Rare ladder with negative gating at each rung.
    fn ladder(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        ilvl: u32,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let class_s = class.as_str().to_string();
        let mut item = self.mk_item(base, ilvl, Rarity::Normal);

        // Normal: only Transmute / Alchemy / Vaal / Hinekora should apply.
        for bad in [
            "OrbOfAugmentation",
            "RegalOrb",
            "ExaltedOrb",
            "ChaosOrb",
            "OrbOfAnnulment",
            "DivineOrb",
            "FracturingOrb",
        ] {
            let mut clone = item.clone();
            if self.apply_id(bad, &mut clone, rng).is_ok() {
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "gate_normal",
                    Status::Fail,
                    format!("{bad} applied to Normal item"),
                );
            }
        }
        match self.apply_id("OrbOfTransmutation", &mut item, rng) {
            Ok(()) => {
                if item.rarity != Rarity::Magic || item.prefixes.len() + item.suffixes.len() != 1 {
                    self.push(
                        &class_s,
                        base,
                        ilvl,
                        "ladder_transmute",
                        Status::Fail,
                        format!(
                            "expected Magic+1mod, got {:?} {}p/{}s",
                            item.rarity,
                            item.prefixes.len(),
                            item.suffixes.len()
                        ),
                    );
                } else {
                    self.push(
                        &class_s,
                        base,
                        ilvl,
                        "ladder_transmute",
                        Status::Pass,
                        "Magic + 1 mod",
                    );
                }
                self.invariants(class, base, ilvl, "ladder_transmute", &item.clone());
            }
            Err(e) => {
                // Legitimate only if the pool is empty at this ilvl.
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "ladder_transmute",
                    Status::Warn,
                    format!("transmute failed: {e}"),
                );
                return;
            }
        }

        // Magic: Transmute/Alchemy/Exalt/Chaos must be rejected.
        for bad in [
            "OrbOfTransmutation",
            "OrbOfAlchemy",
            "ExaltedOrb",
            "ChaosOrb",
        ] {
            let mut clone = item.clone();
            if self.apply_id(bad, &mut clone, rng).is_ok() {
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "gate_magic",
                    Status::Fail,
                    format!("{bad} applied to Magic item"),
                );
            }
        }
        if self.apply_id("OrbOfAugmentation", &mut item, rng).is_ok() {
            self.invariants(class, base, ilvl, "ladder_aug", &item.clone());
            if item.prefixes.len() + item.suffixes.len() != 2 {
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "ladder_aug",
                    Status::Fail,
                    format!(
                        "expected 2 mods after aug, got {}",
                        item.prefixes.len() + item.suffixes.len()
                    ),
                );
            }
        }
        match self.apply_id("RegalOrb", &mut item, rng) {
            Ok(()) => {
                self.invariants(class, base, ilvl, "ladder_regal", &item.clone());
                if item.rarity != Rarity::Rare {
                    self.push(
                        &class_s,
                        base,
                        ilvl,
                        "ladder_regal",
                        Status::Fail,
                        format!("expected Rare, got {:?}", item.rarity),
                    );
                }
            }
            Err(e) => {
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "ladder_regal",
                    Status::Warn,
                    format!("regal failed: {e}"),
                );
                return;
            }
        }

        // Rare: Transmute/Aug/Regal/Alchemy must be rejected.
        for bad in [
            "OrbOfTransmutation",
            "OrbOfAugmentation",
            "RegalOrb",
            "OrbOfAlchemy",
        ] {
            let mut clone = item.clone();
            if self.apply_id(bad, &mut clone, rng).is_ok() {
                self.push(
                    &class_s,
                    base,
                    ilvl,
                    "gate_rare",
                    Status::Fail,
                    format!("{bad} applied to Rare item"),
                );
            }
        }
        // Exalt to capacity.
        let mut exalts = 0;
        loop {
            let before = item.prefixes.len() + item.suffixes.len();
            match self.apply_id("ExaltedOrb", &mut item, rng) {
                Ok(()) => {
                    exalts += 1;
                    self.invariants(class, base, ilvl, "ladder_exalt", &item.clone());
                    if item.prefixes.len() + item.suffixes.len() != before + 1 {
                        self.push(
                            &class_s,
                            base,
                            ilvl,
                            "ladder_exalt",
                            Status::Fail,
                            "exalt succeeded without adding a mod",
                        );
                        break;
                    }
                    if exalts > 6 {
                        self.push(
                            &class_s,
                            base,
                            ilvl,
                            "ladder_exalt",
                            Status::Fail,
                            "more than 6 exalts applied",
                        );
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if item.prefixes.len() + item.suffixes.len() > 6 {
            self.push(
                &class_s,
                base,
                ilvl,
                "ladder_exalt",
                Status::Fail,
                "item exceeded 6 explicit mods",
            );
        }
        // Divine / Chaos / Annul on the rare.
        let mut clone = item.clone();
        if let Err(e) = self.apply_id("DivineOrb", &mut clone, rng) {
            self.push(
                &class_s,
                base,
                ilvl,
                "ladder_divine",
                Status::Fail,
                format!("divine on rare failed: {e}"),
            );
        }
        let mut clone = item.clone();
        match self.apply_id("ChaosOrb", &mut clone, rng) {
            Ok(()) => self.invariants(class, base, ilvl, "ladder_chaos", &clone),
            Err(e) => self.push(
                &class_s,
                base,
                ilvl,
                "ladder_chaos",
                Status::Warn,
                format!("chaos on rare failed: {e}"),
            ),
        }
        let mut clone = item.clone();
        let before = clone.prefixes.len() + clone.suffixes.len();
        if before > 0 {
            match self.apply_id("OrbOfAnnulment", &mut clone, rng) {
                Ok(()) => {
                    if clone.prefixes.len() + clone.suffixes.len() != before - 1 {
                        self.push(
                            &class_s,
                            base,
                            ilvl,
                            "ladder_annul",
                            Status::Fail,
                            "annul did not remove exactly 1 mod",
                        );
                    }
                }
                Err(e) => self.push(
                    &class_s,
                    base,
                    ilvl,
                    "ladder_annul",
                    Status::Warn,
                    format!("annul failed: {e}"),
                ),
            }
        }
        self.push(
            &class_s,
            base,
            ilvl,
            "ladder",
            Status::Pass,
            format!("full ladder ok ({exalts} exalts)"),
        );
    }

    fn alchemy(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let mut item = self.mk_item(base, 82, Rarity::Normal);
        match self.apply_id("OrbOfAlchemy", &mut item, rng) {
            Ok(()) => {
                let n = item.prefixes.len() + item.suffixes.len();
                if item.rarity != Rarity::Rare || n != 4 {
                    self.push(
                        class.as_str(),
                        base,
                        82,
                        "alchemy",
                        Status::Fail,
                        format!("expected Rare+4 mods, got {:?} {n} mods", item.rarity),
                    );
                } else {
                    self.push(
                        class.as_str(),
                        base,
                        82,
                        "alchemy",
                        Status::Pass,
                        "Rare + 4 mods",
                    );
                }
                self.invariants(class, base, 82, "ladder_alchemy", &item);
            }
            Err(e) => self.push(
                class.as_str(),
                base,
                82,
                "alchemy",
                Status::Warn,
                format!("alchemy failed: {e}"),
            ),
        }
    }

    /// Greater/Perfect variants must respect their Min-Modifier-Level floor.
    fn variant_floors(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        // (variant, floor in 0.5)
        let variants: [(&str, u32); 4] = [
            ("GreaterOrbOfTransmutation", 44),
            ("PerfectOrbOfTransmutation", 70),
            ("GreaterExaltedOrb", 35),
            ("PerfectExaltedOrb", 50),
        ];
        for (variant, floor) in variants {
            let mut item = if variant.contains("Transmutation") {
                self.mk_item(base, 82, Rarity::Normal)
            } else {
                let mut it = self.mk_item(base, 82, Rarity::Normal);
                if self.apply_id("OrbOfAlchemy", &mut it, rng).is_err() {
                    continue;
                }
                it
            };
            let before: BTreeSet<String> = item
                .prefixes
                .iter()
                .chain(item.suffixes.iter())
                .map(|r| r.mod_id.as_str().to_string())
                .collect();
            match self.apply_id(variant, &mut item, rng) {
                Ok(()) => {
                    let added: Vec<_> = item
                        .prefixes
                        .iter()
                        .chain(item.suffixes.iter())
                        .filter(|r| !before.contains(r.mod_id.as_str()))
                        .collect();
                    for roll in added {
                        let Some(def) = self.registry.get(&roll.mod_id) else {
                            continue;
                        };
                        if def.required_level < floor {
                            // keep-≥1-tier exception: legal only if no tier of this
                            // group clears the floor at this ilvl for this class.
                            let base_id = poc2_engine::ids::BaseTypeId::from(base);
                            let base_tags = self.base_registry.tags_of(&base_id).to_vec();
                            let group_clears = self
                                .registry
                                .for_class_affix(class, def.affix_type)
                                .iter()
                                .filter_map(|&i| self.registry.at(i))
                                .any(|m| {
                                    m.mod_group.0 == def.mod_group.0
                                        && m.required_level >= floor
                                        && m.required_level <= 82
                                        && m.kind == ModKind::Explicit
                                        && self.registry.inclusive_weight_for_on_base(
                                            m, &base_id, 82, class, &base_tags,
                                        ) > 0.0
                                });
                            if group_clears {
                                self.push(class.as_str(), base, 82, "variant_floor", Status::Fail,
                                    format!("{variant} added {} (req_lvl {} < floor {floor}) though group has eligible tier above floor", def.id.as_str(), def.required_level));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.push(
                        class.as_str(),
                        base,
                        82,
                        "variant_floor",
                        Status::Warn,
                        format!("{variant} failed at ilvl 82: {e}"),
                    );
                }
            }
        }
    }

    /// After Vaal corruption nothing else may apply (except nothing).
    fn corruption_lockout(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let mut item = self.mk_item(base, 82, Rarity::Normal);
        if self.apply_id("OrbOfAlchemy", &mut item, rng).is_err() {
            return;
        }
        if let Err(e) = self.apply_id("VaalOrb", &mut item, rng) {
            self.push(
                class.as_str(),
                base,
                82,
                "vaal",
                Status::Fail,
                format!("vaal on rare failed: {e}"),
            );
            return;
        }
        if !item.corrupted {
            self.push(
                class.as_str(),
                base,
                82,
                "vaal",
                Status::Fail,
                "vaal applied but item not corrupted",
            );
            return;
        }
        let ids = self.all_currency_ids.clone();
        let mut leaked = Vec::new();
        for cid in &ids {
            let mut clone = item.clone();
            if self.apply_id(cid.as_str(), &mut clone, rng).is_ok() {
                // Corrupted essences are the single legal exception (0.4+).
                if !cid.as_str().contains("Corrupted") {
                    leaked.push(cid.as_str().to_string());
                }
            }
        }
        if leaked.is_empty() {
            self.push(
                class.as_str(),
                base,
                82,
                "corruption_lockout",
                Status::Pass,
                "all currencies rejected post-corruption",
            );
        } else {
            self.push(
                class.as_str(),
                base,
                82,
                "corruption_lockout",
                Status::Fail,
                format!(
                    "{} currencies applied to corrupted item: {}",
                    leaked.len(),
                    leaked.join(", ")
                ),
            );
        }
    }

    /// Uniques: per the 0.5 ruleset only Divine (values), Vaal (corruption)
    /// — and possibly Verisium Alloys (pending poe2db verdict) — may touch them.
    fn unique_gating(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let item = self.mk_item(base, 82, Rarity::Unique);
        let ids = self.all_currency_ids.clone();
        let mut allowed = Vec::new();
        for cid in &ids {
            let mut clone = item.clone();
            if self.apply_id(cid.as_str(), &mut clone, rng).is_ok() {
                allowed.push(cid.as_str().to_string());
            }
        }
        let alloy_ids: BTreeSet<String> = self
            .resolver
            .alloys()
            .iter()
            .map(|a| a.id.as_str().to_string())
            .collect();
        let catalyst_ids: BTreeSet<String> = self
            .catalyst_ids
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        let unexpected: Vec<_> = allowed
            .iter()
            .filter(|a| {
                a.as_str() != "DivineOrb"
                    && a.as_str() != "VaalOrb"
                    && !alloy_ids.contains(a.as_str())
                    && !catalyst_ids.contains(a.as_str())
            })
            .cloned()
            .collect();
        let catalysts_allowed: Vec<_> = allowed
            .iter()
            .filter(|a| catalyst_ids.contains(a.as_str()))
            .cloned()
            .collect();
        if !catalysts_allowed.is_empty() {
            self.push(
                class.as_str(),
                base,
                82,
                "unique_catalysts",
                Status::Info,
                format!(
                    "catalysts allowed on unique (poe2db verdict pending): {}",
                    catalysts_allowed.join(", ")
                ),
            );
        }
        let alloys_allowed: Vec<_> = allowed
            .iter()
            .filter(|a| alloy_ids.contains(a.as_str()))
            .cloned()
            .collect();
        if !unexpected.is_empty() {
            self.push(
                class.as_str(),
                base,
                82,
                "unique_gating",
                Status::Fail,
                format!(
                    "non-Divine/Vaal currencies applied to Unique: {}",
                    unexpected.join(", ")
                ),
            );
        } else {
            self.push(
                class.as_str(),
                base,
                82,
                "unique_gating",
                Status::Pass,
                format!("allowed on unique: {}", allowed.join(", ")),
            );
        }
        // Record verisium-on-unique behavior for the poe2db verdict.
        self.push(
            class.as_str(),
            base,
            82,
            "unique_verisium",
            Status::Info,
            if alloys_allowed.is_empty() {
                "alloys rejected on unique (current engine gating: Rare-only)".to_string()
            } else {
                format!("alloys allowed on unique: {}", alloys_allowed.join(", "))
            },
        );
    }

    fn fracturing(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        // 4+ explicit mods required.
        let mut item = self.mk_item(base, 82, Rarity::Normal);
        if self.apply_id("OrbOfAlchemy", &mut item, rng).is_err() {
            return;
        }
        let n = item.prefixes.len() + item.suffixes.len();
        let mut clone = item.clone();
        let res = self.apply_id("FracturingOrb", &mut clone, rng);
        if n >= 4 {
            match res {
                Ok(()) => {
                    let fractured = clone
                        .prefixes
                        .iter()
                        .chain(clone.suffixes.iter())
                        .filter(|r| r.is_fractured)
                        .count();
                    if fractured == 1 {
                        self.push(
                            class.as_str(),
                            base,
                            82,
                            "fracturing",
                            Status::Pass,
                            "1 mod fractured",
                        );
                    } else {
                        self.push(
                            class.as_str(),
                            base,
                            82,
                            "fracturing",
                            Status::Fail,
                            format!("{fractured} mods fractured, expected 1"),
                        );
                    }
                }
                Err(e) => self.push(
                    class.as_str(),
                    base,
                    82,
                    "fracturing",
                    Status::Fail,
                    format!("fracture on 4-mod rare failed: {e}"),
                ),
            }
        }
        // <4 mods must be rejected.
        let mut small = self.mk_item(base, 82, Rarity::Normal);
        if self.apply_id("OrbOfTransmutation", &mut small, rng).is_ok()
            && self.apply_id("RegalOrb", &mut small, rng).is_ok()
        {
            // 2-3 mods rare
            let mut clone = small.clone();
            if self.apply_id("FracturingOrb", &mut clone, rng).is_ok() {
                self.push(
                    class.as_str(),
                    base,
                    82,
                    "fracturing_gate",
                    Status::Fail,
                    format!(
                        "fracture applied to {}-mod rare",
                        small.prefixes.len() + small.suffixes.len()
                    ),
                );
            }
        }
    }

    fn bones(&mut self, class: &ItemClassId, base: &str, _bn: &str, rng: &mut Xoshiro256PlusPlus) {
        let class_s = class.as_str().to_string();
        // Does this class have any desecrated mods at all?
        let pool: Vec<_> = self
            .registry
            .for_class_affix(class, AffixType::Prefix)
            .iter()
            .chain(
                self.registry
                    .for_class_affix(class, AffixType::Suffix)
                    .iter(),
            )
            .filter_map(|&i| self.registry.at(i))
            .filter(|m| m.kind == ModKind::Desecrated)
            .cloned()
            .collect();

        let mut item = self.mk_item(base, 82, Rarity::Normal);
        if self.apply_id("OrbOfAlchemy", &mut item, rng).is_err() {
            return;
        }
        // Subtype → class matrix: Rib = armour, Jawbone = weapons,
        // Collarbone = jewellery, Cranium = jewels. Find this class's bone.
        let subtype_bone = [
            "PreservedRib",
            "PreservedJawbone",
            "PreservedCollarbone",
            "PreservedCranium",
            "AlteredCollarbone",
        ]
        .iter()
        .find(|b| {
            let mut probe = item.clone();
            self.apply_id(b, &mut probe, rng).is_ok()
        })
        .copied();
        let Some(working_bone) = subtype_bone else {
            if pool.is_empty() {
                self.push(
                    &class_s,
                    base,
                    82,
                    "bone_apply",
                    Status::Pass,
                    "no desecrated pool for class; bones correctly rejected",
                );
                return;
            }
            self.push(
                &class_s,
                base,
                82,
                "bone_apply",
                if pool.is_empty() {
                    Status::Info
                } else {
                    Status::Fail
                },
                format!(
                    "no bone subtype applies to rare {class_s} ({} desecrated mods in pool)",
                    pool.len()
                ),
            );
            return;
        };
        let mut it = item.clone();
        match self.apply_id(working_bone, &mut it, rng) {
            Ok(()) => {
                if it.hidden_desecrated.is_none() {
                    self.push(
                        &class_s,
                        base,
                        82,
                        "bone_apply",
                        Status::Fail,
                        "bone applied but no hidden desecrated slot",
                    );
                    return;
                }
                // Second bone while one is hidden must fail.
                let mut second = it.clone();
                if self.apply_id(working_bone, &mut second, rng).is_ok() {
                    self.push(
                        &class_s,
                        base,
                        82,
                        "bone_second_hidden",
                        Status::Fail,
                        "second bone applied while hidden slot occupied",
                    );
                }
                let opts = sample_reveal_options(&it, &pool, 3, rng);
                if opts.is_empty() {
                    self.push(
                        &class_s,
                        base,
                        82,
                        "bone_reveal_options",
                        if pool.is_empty() {
                            Status::Warn
                        } else {
                            Status::Fail
                        },
                        format!(
                            "no reveal options (class desecrated pool: {} mods)",
                            pool.len()
                        ),
                    );
                    return;
                }
                let chosen = opts[0].clone();
                match reveal_at_well_of_souls(&mut it, &pool, &chosen, rng) {
                    Ok(()) => {
                        let has = it
                            .prefixes
                            .iter()
                            .chain(it.suffixes.iter())
                            .any(|r| r.mod_id == chosen && r.kind == ModKind::Desecrated);
                        if has {
                            self.push(
                                &class_s,
                                base,
                                82,
                                "bone_reveal",
                                Status::Pass,
                                format!("revealed {} ({} options)", chosen.as_str(), opts.len()),
                            );
                        } else {
                            self.push(
                                &class_s,
                                base,
                                82,
                                "bone_reveal",
                                Status::Fail,
                                "revealed mod not present on item",
                            );
                        }
                        // 0.5: max 1 desecrated mod — another bone must fail.
                        let mut third = it.clone();
                        if self.apply_id(working_bone, &mut third, rng).is_ok() {
                            self.push(&class_s, base, 82, "bone_desecrated_cap", Status::Fail, "bone applied to item already carrying a desecrated mod (0.5 cap = 1)");
                        }
                    }
                    Err(e) => self.push(
                        &class_s,
                        base,
                        82,
                        "bone_reveal",
                        Status::Fail,
                        format!("reveal failed: {e}"),
                    ),
                }
            }
            Err(e) => {
                self.push(
                    &class_s,
                    base,
                    82,
                    "bone_apply",
                    Status::Fail,
                    format!("probe-validated bone {working_bone} failed on second apply: {e}"),
                );
            }
        }
        // Gnawed bones: ≤ ilvl 64 only (use this class's working subtype).
        let gnawed = working_bone.replace("Preserved", "Gnawed");
        let mut high = self.mk_item(base, 82, Rarity::Normal);
        if self.apply_id("OrbOfAlchemy", &mut high, rng).is_ok() {
            let mut clone = high.clone();
            if self.apply_id(&gnawed, &mut clone, rng).is_ok() {
                self.push(
                    &class_s,
                    base,
                    82,
                    "bone_gnawed_gate",
                    Status::Fail,
                    "Gnawed bone applied to ilvl-82 item (cap is ilvl 64)",
                );
            }
        }
    }

    fn essence_sweep(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let class_s = class.as_str().to_string();
        let essences = self.essences.clone();
        let mut applicable = 0_usize;
        let mut wrong_class_applied = Vec::new();
        let mut applicable_failed = Vec::new();
        let pool = self
            .base_registry
            .get(&poc2_engine::ids::BaseTypeId::from(base))
            .map_or(poc2_engine::AttributePool::None, |b| b.attribute_pool);
        for e in &essences {
            let resolved = e.resolve_target(class, pool).cloned();
            let Some(def) = resolved.as_ref().and_then(|m| self.registry.get(m)) else {
                // No per-class target → essence illegal here; verify it rejects.
                let mut item = self.mk_item(base, 82, Rarity::Normal);
                let prep = if e.quality.is_remove_add() {
                    "OrbOfAlchemy"
                } else {
                    "OrbOfTransmutation"
                };
                if self.apply_id(prep, &mut item, rng).is_ok()
                    && self.apply_id(e.id.as_str(), &mut item, rng).is_ok()
                {
                    wrong_class_applied.push(e.id.as_str().to_string());
                }
                continue;
            };
            let class_ok =
                def.allowed_item_classes.is_empty() || def.allowed_item_classes.contains(class);
            let mut item = if e.quality.is_remove_add() {
                let mut it = self.mk_item(base, 82, Rarity::Normal);
                if self.apply_id("OrbOfAlchemy", &mut it, rng).is_err() {
                    continue;
                }
                it
            } else {
                let mut it = self.mk_item(base, 82, Rarity::Normal);
                if self.apply_id("OrbOfTransmutation", &mut it, rng).is_err() {
                    continue;
                }
                it
            };
            let res = self.apply_id(e.id.as_str(), &mut item, rng);
            match (class_ok, res) {
                (true, Ok(())) => {
                    applicable += 1;
                    let has = item.prefixes.iter().chain(item.suffixes.iter()).any(|r| {
                        self.registry
                            .get(&r.mod_id)
                            .is_some_and(|d| d.mod_group.0 == def.mod_group.0)
                    });
                    if !has {
                        applicable_failed.push(format!(
                            "{} (applied but target mod group absent)",
                            e.id.as_str()
                        ));
                    }
                    if !e.quality.is_remove_add() && item.rarity != Rarity::Rare {
                        applicable_failed
                            .push(format!("{} (did not promote to Rare)", e.id.as_str()));
                    }
                }
                (true, Err(err)) => {
                    // May be legal (e.g. group collision with rolled mods) — only
                    // flag when the message is not a sensible gate.
                    applicable_failed.push(format!("{} ({err})", e.id.as_str()));
                }
                (false, Ok(())) => wrong_class_applied.push(e.id.as_str().to_string()),
                (false, Err(_)) => {}
            }
        }
        if !wrong_class_applied.is_empty() {
            self.push(
                &class_s,
                base,
                82,
                "essence_class_gate",
                Status::Fail,
                format!(
                    "{} essences applied despite target mod not allowing class: {}",
                    wrong_class_applied.len(),
                    wrong_class_applied.join(", ")
                ),
            );
        }
        if applicable == 0 {
            self.push(
                &class_s,
                base,
                82,
                "essence_coverage",
                Status::Warn,
                "no essence applies to this class",
            );
        } else {
            self.push(
                &class_s,
                base,
                82,
                "essence_coverage",
                Status::Pass,
                format!("{applicable} essences applicable"),
            );
        }
        if !applicable_failed.is_empty() {
            // Group-collision rejections are expected (the prep item may
            // randomly roll the essence's group first) — informational.
            let benign = applicable_failed
                .iter()
                .all(|f| f.contains("mod group exclusivity"));
            self.push(
                &class_s,
                base,
                82,
                "essence_apply",
                if benign { Status::Info } else { Status::Warn },
                format!(
                    "{} applicable essences had issues: {}",
                    applicable_failed.len(),
                    applicable_failed
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
        }
    }

    fn catalyst_sweep(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let class_s = class.as_str().to_string();
        // poe2db 0.5: base catalysts apply to "a ring or amulet" only;
        // "Refined" variants apply to "a jewel" only. Belts take none.
        let expect_base = ["Ring", "Amulet"].contains(&class.as_str());
        let expect_refined = class.as_str() == "Jewel";
        let ids = self.catalyst_ids.clone();
        let mut applied_base = Vec::new();
        let mut applied_refined = Vec::new();
        for cid in &ids {
            let mut item = self.mk_item(base, 82, Rarity::Normal);
            if self.apply_id("OrbOfAlchemy", &mut item, rng).is_err() {
                continue;
            }
            let q_before = item.quality;
            if self.apply_id(cid.as_str(), &mut item, rng).is_ok() {
                if cid.as_str().starts_with("Refined") {
                    applied_refined.push(cid.as_str().to_string());
                } else {
                    applied_base.push(cid.as_str().to_string());
                }
                if item.quality <= q_before {
                    self.push(
                        &class_s,
                        base,
                        82,
                        "catalyst_quality",
                        Status::Fail,
                        format!("{} applied but quality did not increase", cid.as_str()),
                    );
                }
            }
        }
        let base_ok = expect_base != applied_base.is_empty();
        let refined_ok = expect_refined != applied_refined.is_empty();
        if base_ok && refined_ok {
            self.push(
                &class_s,
                base,
                82,
                "catalyst_gate",
                Status::Pass,
                format!(
                    "base: {} refined: {}",
                    applied_base.len(),
                    applied_refined.len()
                ),
            );
        } else {
            self.push(
                &class_s,
                base,
                82,
                "catalyst_gate",
                Status::Fail,
                format!(
                    "expected base={expect_base} refined={expect_refined}; applied base [{}] refined [{}]",
                    applied_base.join(", "),
                    applied_refined.join(", ")
                ),
            );
        }
    }

    fn alloy_sweep(
        &mut self,
        class: &ItemClassId,
        base: &str,
        base_name: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let class_s = class.as_str().to_string();
        let alloys: Vec<_> = self.resolver.alloys().to_vec();
        let mut ok = 0_usize;
        let mut illegal = Vec::new();
        for a in &alloys {
            let class_target = a.target_for_class(class).cloned();
            let base_target = a
                .base_targets
                .iter()
                .any(|(b, _)| base_name.to_lowercase().contains(&b.to_lowercase()));
            let legal = (!a.class_targets.is_empty() && class_target.is_some())
                || (a.class_targets.is_empty() && (a.base_targets.is_empty() || base_target));
            let mut item = self.mk_item(base, 82, Rarity::Normal);
            if self.apply_id("OrbOfAlchemy", &mut item, rng).is_err() {
                continue;
            }
            match (legal, self.apply_id(a.id.as_str(), &mut item, rng)) {
                (true, Ok(())) => {
                    ok += 1;
                    let crafted = item
                        .prefixes
                        .iter()
                        .chain(item.suffixes.iter())
                        .filter(|r| r.kind == ModKind::Crafted)
                        .count();
                    if crafted != 1 {
                        self.push(
                            &class_s,
                            base,
                            82,
                            "alloy_crafted_mod",
                            Status::Fail,
                            format!(
                                "{}: {crafted} crafted mods after alloy, expected 1",
                                a.id.as_str()
                            ),
                        );
                    }
                    // 0.5: 1 crafted mod cap — second alloy must not yield 2.
                    let mut second = item.clone();
                    if self.apply_id(a.id.as_str(), &mut second, rng).is_ok() {
                        let crafted2 = second
                            .prefixes
                            .iter()
                            .chain(second.suffixes.iter())
                            .filter(|r| r.kind == ModKind::Crafted)
                            .count();
                        if crafted2 > 1 {
                            self.push(
                                &class_s,
                                base,
                                82,
                                "alloy_crafted_cap",
                                Status::Fail,
                                format!(
                                    "{}: {crafted2} crafted mods after second alloy (cap 1)",
                                    a.id.as_str()
                                ),
                            );
                        }
                    }
                }
                (true, Err(_)) => { /* legal-but-failed can be group collision; not a hard fail */ }
                (false, Ok(())) => illegal.push(a.id.as_str().to_string()),
                (false, Err(_)) => {}
            }
        }
        if !illegal.is_empty() {
            self.push(
                &class_s,
                base,
                82,
                "alloy_class_gate",
                Status::Fail,
                format!(
                    "{} alloys applied without a class/base target: {}",
                    illegal.len(),
                    illegal
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        self.push(
            &class_s,
            base,
            82,
            "alloy_coverage",
            if ok > 0 { Status::Pass } else { Status::Info },
            format!("{ok} alloys/emotions applicable"),
        );
    }

    /// Advisor legality: plan() per rarity rung; every ApplyCurrency
    /// recommendation must be resolvable and applicable to the item.
    fn advisor(
        &mut self,
        class: &ItemClassId,
        base: &str,
        _bn: &str,
        rng: &mut Xoshiro256PlusPlus,
    ) {
        let class_s = class.as_str().to_string();
        // Pick the most frequent prefix + suffix concepts in this class's pool.
        let mut freq: BTreeMap<(bool, String), usize> = BTreeMap::new();
        for affix in [AffixType::Prefix, AffixType::Suffix] {
            for &idx in self.registry.for_class_affix(class, affix) {
                let Some(m) = self.registry.at(idx) else {
                    continue;
                };
                if m.kind != ModKind::Explicit
                    || m.flags.intersects(
                        ModFlags::ESSENCE_ONLY
                            | ModFlags::DESECRATED_ONLY
                            | ModFlags::CORRUPTED_ONLY,
                    )
                {
                    continue;
                }
                for c in &m.concept_set {
                    *freq
                        .entry((affix == AffixType::Prefix, c.as_str().to_string()))
                        .or_default() += 1;
                }
            }
        }
        let top = |affix: AffixType| -> Option<String> {
            freq.iter()
                .filter(|((a, _), _)| *a == (affix == AffixType::Prefix))
                .max_by_key(|(_, n)| **n)
                .map(|((_, c), _)| c.clone())
        };
        let mut target = Target::default();
        if let Some(c) = top(AffixType::Prefix) {
            target.prefixes.push(TargetSpec {
                concept: Some(poc2_engine::ConceptId::from(c)),
                concept_any: vec![],
                affix: Some(AffixType::Prefix),
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            });
        }
        if let Some(c) = top(AffixType::Suffix) {
            target.suffixes.push(TargetSpec {
                concept: Some(poc2_engine::ConceptId::from(c)),
                concept_any: vec![],
                affix: Some(AffixType::Suffix),
                count: 1,
                min_tier: None,
                allow_hybrid: true,
            });
        }
        if target.prefixes.is_empty() && target.suffixes.is_empty() {
            self.push(
                &class_s,
                base,
                82,
                "advisor_goal",
                Status::Warn,
                "no concepts in class pool — cannot build a goal",
            );
            return;
        }
        let goal = Goal::new(
            target,
            poc2_market::DivEquiv {
                min: 0.0,
                expected: 10.0,
                max: 20.0,
            },
        );

        // Build the rung items: Normal, Magic(1), Rare(4), corrupted Rare.
        let normal = self.mk_item(base, 82, Rarity::Normal);
        let mut magic = normal.clone();
        let _ = self.apply_id("OrbOfTransmutation", &mut magic, rng);
        let mut rare = normal.clone();
        let _ = self.apply_id("OrbOfAlchemy", &mut rare, rng);
        let mut corrupted = rare.clone();
        let _ = self.apply_id("VaalOrb", &mut corrupted, rng);

        for (stage, item) in [
            ("normal", &normal),
            ("magic", &magic),
            ("rare", &rare),
            ("corrupted", &corrupted),
        ] {
            let stash = Stash::unlimited();
            let input = PlanInput {
                item: item.clone(),
                goal: goal.clone(),
                rules: &self.rules,
                strategies: &self.strategies,
                registry: &self.registry,
                resolver: &self.resolver,
                valuator: &self.valuator,
                stash: &stash,
                patch: self.patch,
                league: self.league,
                plugin_dispatch: None,
                base_registry: Some(&self.base_registry),
                trained_models: None,
                config: BeamConfig {
                    width: 5,
                    depth: 3,
                    top_n: 5,
                    seed: 7,
                    mc_samples: 10,
                    ..BeamConfig::default()
                },
            };
            let recs = plan(&input);
            if recs.is_empty() && stage != "corrupted" {
                self.push(
                    &class_s,
                    base,
                    82,
                    "advisor_recs",
                    Status::Fail,
                    format!("no recommendations at stage {stage}"),
                );
                continue;
            }
            let mut illegal = Vec::new();
            for r in &recs {
                if let AdvisorAction::ApplyCurrency { currency, .. } = &r.action {
                    let mut clone = item.clone();
                    if let Err(e) = self.apply_id(currency.as_str(), &mut clone, rng) {
                        illegal.push(format!("{} ({e})", currency.as_str()));
                    }
                }
            }
            if illegal.is_empty() {
                self.push(
                    &class_s,
                    base,
                    82,
                    "advisor_legal",
                    Status::Pass,
                    format!("stage {stage}: {} recs all legal", recs.len()),
                );
            } else {
                self.push(
                    &class_s,
                    base,
                    82,
                    "advisor_legal",
                    Status::Fail,
                    format!(
                        "stage {stage}: illegal recommendations: {}",
                        illegal.join("; ")
                    ),
                );
            }
        }
    }
}
