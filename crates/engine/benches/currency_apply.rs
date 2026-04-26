//! Currency `apply()` performance baseline (M2.9).
//!
//! Per ADR-0007, the advisor's beam search runs tens of thousands of
//! `apply()` calls per re-plan, so the engine's hot path must stay
//! sub-millisecond. This bench tracks four representative operations:
//!
//! - `transmute_on_normal` — single-mod add, the cheapest path
//! - `regal_on_magic` — single-mod add with rarity promotion
//! - `chaos_on_rare` — remove + add (the busiest reroll path)
//! - `divine_on_rare_3p_3s` — re-roll values on a fully-modded item
//!
//! Run with `cargo bench --bench currency_apply -p poc2-engine`.
//! Sample output is written under `target/criterion/`.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use poc2_engine::currency::basic::{ChaosOrb, DivineOrb, OrbOfTransmutation, RegalOrb};
use poc2_engine::ids::{ItemClassId, ModGroupId, ModId, TagId};
use poc2_engine::item::{AffixType, Item, ModRoll, QualityKind, Rarity};
use poc2_engine::mods::{ModDefinition, ModDomain, ModFlags, ModGroup, ModKind, SpawnWeight};
use poc2_engine::omen::OmenSet;
use poc2_engine::patch::{PatchRange, PatchVersion};
use poc2_engine::registry::ModRegistry;
use poc2_engine::{apply_currency, Currency};
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

fn registry_with_n_per_affix(n: usize) -> ModRegistry {
    let mut mods: Vec<ModDefinition> = Vec::with_capacity(2 * n);
    for i in 0..n {
        mods.push(mk_mod(
            &format!("Prefix{i}"),
            &format!("PrefixGroup{i}"),
            AffixType::Prefix,
            "BodyArmour",
        ));
    }
    for i in 0..n {
        mods.push(mk_mod(
            &format!("Suffix{i}"),
            &format!("SuffixGroup{i}"),
            AffixType::Suffix,
            "BodyArmour",
        ));
    }
    ModRegistry::from_mods(mods)
}

fn mk_mod(id: &str, group: &str, affix: AffixType, class: &str) -> ModDefinition {
    ModDefinition {
        id: ModId::from(id),
        name: None,
        mod_group: ModGroup(ModGroupId::from(group)),
        affix_type: affix,
        kind: ModKind::Explicit,
        domain: ModDomain::Item,
        tags: smallvec![],
        concept_set: smallvec![],
        spawn_weights: smallvec![SpawnWeight {
            tag: TagId::from(class),
            weight: 1
        }],
        stats: smallvec![],
        required_level: 1,
        allowed_item_classes: smallvec![ItemClassId::from(class)],
        patch_range: PatchRange::ALL,
        flags: ModFlags::empty(),
        text_template: None,
    }
}

fn empty_normal() -> Item {
    Item {
        base: ItemClassId::from("BodyArmour").as_str().into(),
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
    }
}

fn magic_with_one_prefix() -> Item {
    let mut item = empty_normal();
    item.rarity = Rarity::Magic;
    item.prefixes.push(ModRoll {
        mod_id: ModId::from("Prefix0"),
        affix_type: AffixType::Prefix,
        kind: ModKind::Explicit,
        values: smallvec![],
        is_fractured: false,
    });
    item
}

fn rare_3p_3s() -> Item {
    let mut item = empty_normal();
    item.rarity = Rarity::Rare;
    for i in 0..3 {
        item.prefixes.push(ModRoll {
            mod_id: ModId::from(format!("Prefix{i}").as_str()),
            affix_type: AffixType::Prefix,
            kind: ModKind::Explicit,
            values: smallvec![10.0, 20.0],
            is_fractured: false,
        });
    }
    for i in 0..3 {
        item.suffixes.push(ModRoll {
            mod_id: ModId::from(format!("Suffix{i}").as_str()),
            affix_type: AffixType::Suffix,
            kind: ModKind::Explicit,
            values: smallvec![10.0, 20.0],
            is_fractured: false,
        });
    }
    item
}

fn bench_currencies(c: &mut Criterion) {
    let registry = registry_with_n_per_affix(50);
    let patch = PatchVersion::PATCH_0_4_0;

    c.bench_function("apply_transmute_on_normal", |b| {
        let trans = OrbOfTransmutation::new();
        let base_item = empty_normal();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xfeed);
        b.iter_batched(
            || base_item.clone(),
            |mut item| {
                let mut omens = OmenSet::new();
                let _ = apply_currency(&trans, &mut item, &registry, &mut rng, patch, &mut omens);
                black_box(item);
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("apply_regal_on_magic", |b| {
        let regal = RegalOrb::new();
        let base_item = magic_with_one_prefix();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xface);
        b.iter_batched(
            || base_item.clone(),
            |mut item| {
                let mut omens = OmenSet::new();
                let _ = apply_currency(&regal, &mut item, &registry, &mut rng, patch, &mut omens);
                black_box(item);
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("apply_chaos_on_rare_3p3s", |b| {
        let chaos = ChaosOrb::new();
        let base_item = rare_3p_3s();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xc0de);
        b.iter_batched(
            || base_item.clone(),
            |mut item| {
                let mut omens = OmenSet::new();
                let _ = apply_currency(&chaos, &mut item, &registry, &mut rng, patch, &mut omens);
                black_box(item);
            },
            BatchSize::SmallInput,
        );
    });

    c.bench_function("apply_divine_on_rare_3p3s", |b| {
        let divine = DivineOrb::new();
        let base_item = rare_3p_3s();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xb00f);
        b.iter_batched(
            || base_item.clone(),
            |mut item| {
                let mut omens = OmenSet::new();
                let _ = apply_currency(&divine, &mut item, &registry, &mut rng, patch, &mut omens);
                black_box(item);
            },
            BatchSize::SmallInput,
        );
    });

    // Direct trait-object dispatch — what the advisor's resolver path actually does.
    c.bench_function("apply_via_trait_object_dispatch", |b| {
        let trans: Box<dyn Currency> = Box::new(OrbOfTransmutation::new());
        let base_item = empty_normal();
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x1337);
        b.iter_batched(
            || base_item.clone(),
            |mut item| {
                let mut omens = OmenSet::new();
                let _ = apply_currency(
                    trans.as_ref(),
                    &mut item,
                    &registry,
                    &mut rng,
                    patch,
                    &mut omens,
                );
                black_box(item);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(currency_apply, bench_currencies);
criterion_main!(currency_apply);
