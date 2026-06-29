//! Probe: Refined catalysts must apply to rare Jewels (poe2db 0.5).
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use smallvec::smallvec;

use poc2_engine::{
    apply_currency_with_bases, BaseRegistry, CurrencyId, CurrencyResolver, DefaultCurrencyResolver,
    Item, ModRegistry, OmenSet, QualityKind, Rarity, ReleaseState,
};

fn bundle_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let pb = std::path::PathBuf::from(home).join(".config/poc2/bundles/poc2.bundle.json.gz");
    pb.exists().then_some(pb)
}

#[test]
fn refined_catalyst_applies_to_rare_jewel() {
    let Some(path) = bundle_path() else { return };
    let bundle = poc2_data::io::read_bundle(&path).expect("bundle loads");
    let registry = ModRegistry::from_mods(bundle.mods.clone(), bundle.weights.clone());
    let base_registry = BaseRegistry::from_bases(bundle.base_items.clone());
    let resolver = DefaultCurrencyResolver::new().with_catalysts(bundle.catalyst_catalogue());
    let jewel = bundle
        .base_items
        .iter()
        .filter(|b| b.item_class.as_str() == "Jewel" && b.release_state == ReleaseState::Released)
        .max_by_key(|b| b.drop_level)
        .expect("jewel base");
    eprintln!("jewel base: {} ({})", jewel.name, jewel.id.as_str());
    let mut item = Item {
        base: jewel.id.as_str().into(),
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
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
    let mut omens = OmenSet::new();
    let alch = resolver.resolve(&CurrencyId::from("OrbOfAlchemy")).unwrap();
    let alch_r = apply_currency_with_bases(
        alch.as_ref(),
        &mut item,
        &registry,
        &base_registry,
        &mut rng,
        bundle.header.game_patch,
        &mut omens,
    );
    eprintln!(
        "alchemy on jewel: {alch_r:?}; mods={}p/{}s",
        item.prefixes.len(),
        item.suffixes.len()
    );
    if alch_r.is_err() {
        // fall back: transmute + regal to reach Rare
        for c in ["OrbOfTransmutation", "RegalOrb"] {
            let cur = resolver.resolve(&CurrencyId::from(c)).unwrap();
            let r = apply_currency_with_bases(
                cur.as_ref(),
                &mut item,
                &registry,
                &base_registry,
                &mut rng,
                bundle.header.game_patch,
                &mut omens,
            );
            eprintln!("{c}: {r:?}");
        }
    }
    let refined = resolver
        .resolve(&CurrencyId::from("RefinedFleshCatalyst"))
        .expect("RefinedFleshCatalyst resolves");
    let r = apply_currency_with_bases(
        refined.as_ref(),
        &mut item,
        &registry,
        &base_registry,
        &mut rng,
        bundle.header.game_patch,
        &mut omens,
    );
    eprintln!("refined on jewel: {r:?} quality={}", item.quality);
    assert!(
        r.is_ok(),
        "RefinedFleshCatalyst must apply to a rare Jewel: {r:?}"
    );
}
