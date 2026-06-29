//! Probe: Greater Orb of Transmutation (0.5 floor 44) on an ilvl-82 Bow.
//!
//! The `IncreasedAttackSpeed` group's above-floor tiers (req 45/60/77) carry
//! `ranged: 0` spawn weights — bows cannot roll them — so the
//! keep-≥1-tier exception must add back exactly ONE sub-floor tier: the
//! strongest one rollable on the base (req 37). Observing req-1 tiers in the
//! M14 audit suggested the exception may pick a weaker tier; this probe
//! pins the distribution. Skips when no bundle is on disk.

use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

use poc2_engine::{
    apply_currency_with_bases, BaseRegistry, CurrencyId, CurrencyResolver, DefaultCurrencyResolver,
    Item, ModRegistry, OmenSet, QualityKind, Rarity, ReleaseState,
};

fn bundle_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("POC2_BUNDLE") {
        let pb = std::path::PathBuf::from(p);
        return pb.exists().then_some(pb);
    }
    let home = std::env::var("HOME").ok()?;
    let pb = std::path::PathBuf::from(home).join(".config/poc2/bundles/poc2.bundle.json.gz");
    pb.exists().then_some(pb)
}

#[test]
fn greater_transmute_floor_exception_picks_strongest_subfloor_tier() {
    let Some(path) = bundle_path() else {
        eprintln!("floor_exception_probe: no bundle on disk; skipping.");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).expect("bundle loads");
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let resolver = DefaultCurrencyResolver::new();
    let currency = resolver
        .resolve(&CurrencyId::from("GreaterOrbOfTransmutation"))
        .expect("greater transmute resolves");

    // Highest drop-level released Bow base.
    let bow = bundle
        .base_items
        .iter()
        .filter(|b| {
            b.item_class.as_str() == "Bow"
                && b.release_state == ReleaseState::Released
                && !b.name.starts_with("Runemastered")
        })
        .max_by_key(|b| b.drop_level)
        .expect("a released bow base");

    let fresh = || Item {
        base: bow.id.as_str().into(),
        ilvl: 82,
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
    };

    let mut by_mod: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    let mut subfloor_violations: Vec<String> = Vec::new();
    const FLOOR: u32 = 44;
    const TRIALS: u64 = 2000;

    for seed in 0..TRIALS {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let mut omens = OmenSet::new();
        let mut item = fresh();
        if apply_currency_with_bases(
            currency.as_ref(),
            &mut item,
            &registry,
            &base_registry,
            &mut rng,
            bundle.header.game_patch,
            &mut omens,
        )
        .is_err()
        {
            continue;
        }
        for roll in item.prefixes.iter().chain(item.suffixes.iter()) {
            let Some(def) = registry.get(&roll.mod_id) else {
                continue;
            };
            let e = by_mod
                .entry(def.id.as_str().to_string())
                .or_insert((0, def.required_level));
            e.0 += 1;
            if def.required_level < FLOOR {
                // Exception-legal only if it is the STRONGEST sub-floor tier
                // of its group that the base can roll. Find any same-group
                // tier with higher required_level (still sub-floor, ≤82)
                // carrying weight on this base.
                let base_id = poc2_engine::ids::BaseTypeId::from(bow.id.as_str());
                let tags = base_registry.tags_of(&base_id).to_vec();
                let class = poc2_engine::ids::ItemClassId::from("Bow");
                let stronger_subfloor_exists = registry
                    .for_class_affix(&class, def.affix_type)
                    .iter()
                    .filter_map(|&i| registry.at(i))
                    .any(|m| {
                        m.mod_group.0 == def.mod_group.0
                            && m.required_level > def.required_level
                            && m.required_level < FLOOR
                            && registry.inclusive_weight_for_on_base(m, &base_id, 82, &class, &tags)
                                > 0.0
                    });
                if stronger_subfloor_exists {
                    subfloor_violations.push(format!(
                        "seed {seed}: {} (req {}) added though a stronger sub-floor tier exists",
                        def.id.as_str(),
                        def.required_level
                    ));
                }
            }
        }
    }

    let mut lines: Vec<_> = by_mod
        .iter()
        .map(|(id, (n, rl))| format!("{n:5}  req {rl:3}  {id}"))
        .collect();
    lines.sort();
    eprintln!("--- added-mod distribution over {TRIALS} trials ---");
    for l in lines {
        eprintln!("{l}");
    }
    assert!(
        subfloor_violations.is_empty(),
        "floor exception picked weaker sub-floor tiers ({} cases), e.g.:\n{}",
        subfloor_violations.len(),
        subfloor_violations
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Diagnostic: per-tier own + inclusive weights for IncreasedAttackSpeed on
/// the probe's bow base. Run with --nocapture to inspect.
#[test]
fn dump_attack_speed_weights_on_bow() {
    let Some(path) = bundle_path() else {
        eprintln!("no bundle; skipping");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).expect("bundle loads");
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let bow = bundle
        .base_items
        .iter()
        .filter(|b| {
            b.item_class.as_str() == "Bow"
                && b.release_state == ReleaseState::Released
                && !b.name.starts_with("Runemastered")
        })
        .max_by_key(|b| b.drop_level)
        .expect("bow base");
    let base_id = poc2_engine::ids::BaseTypeId::from(bow.id.as_str());
    let tags = base_registry.tags_of(&base_id).to_vec();
    let class = poc2_engine::ids::ItemClassId::from("Bow");
    eprintln!(
        "base: {} ({}) tags={:?}",
        bow.name,
        bow.id.as_str(),
        tags.iter().map(|t| t.as_str()).collect::<Vec<_>>()
    );
    for i in 1..=8 {
        let id = poc2_engine::ids::ModId::from(format!("LocalIncreasedAttackSpeed{i}").as_str());
        let Some(def) = registry.get(&id) else {
            continue;
        };
        let own = registry.weight_for_on_base(&id, &base_id, 82, &class, &tags);
        let incl = registry.inclusive_weight_for_on_base(def, &base_id, 82, &class, &tags);
        eprintln!(
            "T{i} req={:3} tier={:?}: own={own:8.3} inclusive={incl:8.3}",
            def.required_level, def.tier
        );
    }
}

/// Diagnostic 2: which IncreasedAttackSpeed tiers does the (Bow, Suffix)
/// class-affix index actually surface, and with what filter outcomes?
#[test]
fn dump_class_affix_index_for_bow_suffix() {
    use poc2_engine::mods::ModFlags;
    let Some(path) = bundle_path() else {
        eprintln!("no bundle; skipping");
        return;
    };
    let bundle = poc2_data::io::read_bundle(&path).expect("bundle loads");
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let bow = bundle
        .base_items
        .iter()
        .filter(|b| {
            b.item_class.as_str() == "Bow"
                && b.release_state == ReleaseState::Released
                && !b.name.starts_with("Runemastered")
        })
        .max_by_key(|b| b.drop_level)
        .expect("bow base");
    let base_id = poc2_engine::ids::BaseTypeId::from(bow.id.as_str());
    let tags = base_registry.tags_of(&base_id).to_vec();
    let class = poc2_engine::ids::ItemClassId::from("Bow");
    let excludes = ModFlags::ESSENCE_ONLY
        .union(ModFlags::DESECRATED_ONLY)
        .union(ModFlags::CORRUPTED_ONLY);
    let mut n = 0;
    for &idx in registry.for_class_affix(&class, poc2_engine::AffixType::Suffix) {
        let Some(m) = registry.at(idx) else { continue };
        if m.mod_group.0.as_str() != "IncreasedAttackSpeed" {
            continue;
        }
        n += 1;
        let w = registry.inclusive_weight_for_on_base(m, &base_id, 82, &class, &tags);
        eprintln!(
            "idx={:?} {} req={} kind={:?} flags={:?} patch_ok={} excl={} w={w}",
            idx,
            m.id.as_str(),
            m.required_level,
            m.kind,
            m.flags,
            m.patch_range.contains(bundle.header.game_patch),
            m.flags.intersects(excludes),
        );
    }
    eprintln!("total IncreasedAttackSpeed entries in (Bow,Suffix) index: {n}");
}
