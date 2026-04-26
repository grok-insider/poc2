# Engine Domain Model

> Reference for the types in `crates/engine`. Read alongside [`31-engine-algorithms.md`](31-engine-algorithms.md) (algorithms) and [`11-game-mechanics.md`](11-game-mechanics.md) (game-side concepts).

## Type-level layout

```
                ┌────────────────┐
                │     Bundle     │ (poc2-data)
                └────────┬───────┘
                         │ deserialized into
                         ▼
            ┌──────────────────────────┐
            │     ModRegistry          │  built once at engine startup
            │  by_id, by_group,        │
            │  by_class_affix          │
            └────────┬─────────────────┘
                     │ borrowed by
                     ▼
            ┌──────────────────────────┐       ┌──────────────┐
            │      ApplyContext        │  ◄────│   OmenSet    │
            │  registry, rng, patch,   │       │ active omens │
            │  &mut omens              │       └──────────────┘
            └────────┬─────────────────┘
                     │ passed to
                     ▼
            ┌──────────────────────────┐
            │   Currency::apply        │
            │   mutates Item           │
            └────────┬─────────────────┘
                     ▼
            ┌──────────────────────────┐
            │          Item            │  runtime state
            │ rarity, prefixes,        │
            │ suffixes, hidden_des,    │
            │ hinekora_lock, sockets   │
            └──────────────────────────┘
```

## ID newtypes (`crate::ids`)

Every entity has an opaque string identifier wrapped in a newtype. Currently `Box<str>` backed; the M2.9 perf pass will introduce an interner.

| Type | Wraps | Source of truth |
|---|---|---|
| `ModId` | mod id | RePoE-fork mods.json keys |
| `BaseTypeId` | base item id | RePoE-fork base_items.json keys |
| `ItemClassId` | class id | `BodyArmour`, `Boots`, `OneHandSword`, … |
| `TagId` | gameplay tag | `boots`, `int_armour`, `caster`, … |
| `ConceptId` | semantic concept | `EnergyShield`, `Life`, `FireResistance`, … (our taxonomy, not GGG's) |
| `StatId` | raw stat output | `local_energy_shield_+%`, `base_maximum_life`, … |
| `ModGroupId` | mod-group key | `BaseLocalDefencesAndLife`, `Life`, … |
| `CurrencyId` | currency id | `OrbOfTransmutation`, `PerfectExaltedOrb`, … |
| `OmenId` | omen id | `OmenOfSinistralExaltation`, … |
| `EssenceId` | essence id | `PerfectEssenceOfSeeking`, … |

All ID types implement `From<&str>`, `From<String>`, `AsRef<str>`, `Display`, and serde `transparent`. They are NOT interchangeable — `ModId` cannot be passed where `BaseTypeId` is expected.

## Patch versioning (`crate::patch`)

Every entity carries a `PatchRange { min: Option<PatchVersion>, max: Option<PatchVersion> }`. The bundle declares its `game_patch`; `OmenSet::consume` and the various engine queries filter entities to those whose range contains `game_patch`.

`PatchVersion` parses `0.4.0`, `0.4.0c`, `0.5.0`, etc. Subpatch letters are mapped `a..=z → 1..=26`.

Special constants: `PatchVersion::PATCH_0_4_0`, `PatchVersion::PATCH_0_5_0`, `PatchRange::ALL`.

## Mods (`crate::mods`)

```rust
pub struct ModDefinition {
    pub id: ModId,
    pub name: Option<String>,             // "Monk's"
    pub mod_group: ModGroup,              // exclusivity bucket
    pub affix_type: AffixType,            // Prefix | Suffix | Implicit | Enchantment
    pub kind: ModKind,                    // Explicit | Implicit | Enchantment | Desecrated | Corrupted
    pub domain: ModDomain,                // Item | Map | Jewel | AbyssJewel | Atlas | Misc
    pub tags: SmallVec<[TagId; 8]>,
    pub concept_set: SmallVec<[ConceptId; 4]>,    // populated by analyzer
    pub spawn_weights: SmallVec<[SpawnWeight; 6]>,
    pub stats: SmallVec<[ModStat; 4]>,    // multi-entry → potentially hybrid
    pub required_level: u32,
    pub allowed_item_classes: SmallVec<[ItemClassId; 8]>,
    pub patch_range: PatchRange,
    pub flags: ModFlags,                  // LOCAL | ESSENCE_ONLY | DESECRATED_ONLY | HYBRID | CORRUPTED_ONLY
    pub text_template: Option<String>,
}

pub struct ModStat { pub stat_id: StatId, pub min: f64, pub max: f64 }
pub struct SpawnWeight { pub tag: TagId, pub weight: u32 }
```

A `ModDefinition` represents one (group × tier) combination. The "tier ladder" of a group is the set of `ModDefinition`s sharing the same `mod_group`, ordered by `required_level`.

## Items (`crate::item`)

```rust
pub struct Item {
    pub base: BaseTypeId,
    pub ilvl: u32,
    pub rarity: Rarity,
    pub corrupted: bool,
    pub sanctified: bool,
    pub mirrored: bool,
    pub quality: u8,
    pub quality_kind: QualityKind,        // Untagged | Tagged(catalyst tag)
    pub implicits: SmallVec<[ModRoll; 2]>,
    pub prefixes: SmallVec<[ModRoll; 3]>,
    pub suffixes: SmallVec<[ModRoll; 3]>,
    pub enchantments: SmallVec<[ModRoll; 2]>,
    pub hidden_desecrated: Option<HiddenDesecratedSlot>,
    pub sockets: SmallVec<[Socket; 2]>,
    pub hinekora_lock: Option<u64>,       // RNG seed when bound
}

pub struct ModRoll {
    pub mod_id: ModId,
    pub affix_type: AffixType,
    pub kind: ModKind,
    pub values: SmallVec<[f64; 4]>,        // parallels ModDefinition.stats
    pub is_fractured: bool,
}

pub struct HiddenDesecratedSlot {
    pub affix_type: AffixType,             // forced by Sinistral/Dextral Necromancy
    pub bone_size: BoneSize,                // Gnawed | Preserved | Ancient
    pub bone_subtype: BoneSubtype,          // Jawbone | Rib | Cranium | Collarbone
    pub abyss_lord: Option<AbyssLord>,      // Kurgal | Amanamu | Ulaman, if Lord-omen was active
}
```

### Item helpers

- `item.visible_explicit_mod_count()` — prefix + suffix count, excludes hidden
- `item.fracturing_eligibility_count()` — same + hidden_desecrated (the 4-mod check)
- `item.fracture_targets()` — iterator over visible non-fractured mods (the sample space for Fracturing Orb)
- `item.has_fractured()` — quick check
- `item.is_modifiable()` — false when corrupted / sanctified / mirrored

## Registry (`crate::registry`)

`ModRegistry::from_mods(Vec<ModDefinition>)` builds three indices:

- `by_id: AHashMap<ModId, ModIndex>` — O(1) ID lookup
- `by_group: AHashMap<ModGroupId, [ModIndex]>` — mod-group ladder for tier walks
- `by_class_affix: AHashMap<(ItemClassId, AffixType), Vec<ModIndex>>` — primary `apply()` query

Read-only and `Send + Sync`, shared across advisor beam-search workers.

## Currency trait (`crate::currency`)

```rust
pub trait Currency: Debug + Send + Sync {
    fn id(&self) -> &CurrencyId;
    fn name(&self) -> &'static str;
    fn apply(&self, item: &mut Item, ctx: &mut ApplyContext<'_>) -> EngineResult<ApplyOutcome>;
}

pub struct ApplyContext<'a> {
    pub registry: &'a ModRegistry,
    pub rng: &'a mut dyn RngCore,
    pub patch: PatchVersion,
    pub omens: &'a mut OmenSet,
}
```

Implementations live in:
- `currency::basic` — Transmute / Aug / Alch / Regal / Exalt / Chaos / Annul / Divine / Vaal + Greater/Perfect variants
- `currency::fracturing` — Fracturing Orb
- `currency::bone` — Bone + reveal_at_well_of_souls + sample_reveal_options
- `currency::hinekora` — Hinekora's Lock
- `currency::essence` — Essence (Lesser/Normal/Greater/Perfect/Corrupted)

## Top-level orchestration (`crate::engine`)

Three free functions wrap `Currency::apply` and handle Hinekora's Lock + omen rollback:

- `apply_currency(currency, item, registry, rng, patch, omens)` — commits with lock-aware RNG; consumes lock on success; rolls back omens on failure
- `preview_currency(currency, item, registry, rng, patch, omens)` — clones the item, applies, returns the post-apply state without mutating the original
- `commit_with_preview(currency, item, …, accept_fn)` — preview + conditional commit; commit byte-matches the preview because both use the same lock seed

## Omens (`crate::omen`)

```rust
pub enum OmenEffect {
    AffixOnly(AffixType),                    // Sinistral/Dextral *
    GreaterExaltation,
    Whittling,
    Light,
    AbyssalEchoes,
    PreventNoChange,
    Sanctification,
    Blessed,
    CatalystingExaltation,
    LordTarget(AbyssLord),
    HomogenisingTagMatch,                    // disabled in 0.4
}

pub struct Omen { pub id: OmenId, pub effect: OmenEffect, pub patch_range: PatchRange }
pub struct OmenSet { /* SmallVec<[Omen; 4]> */ }
```

Pre-built omens via `Omen::sinistral_exaltation()`, `Omen::dextral_necromancy()`, `Omen::greater_exaltation()`, etc. Each declares the right `patch_range` (Homogenising omens are pinned to `patch_max = 0.3.x`).

`OmenSet::consume_*` helpers are typed and patch-aware: `consume_affix_only`, `consume_greater_exaltation`, `consume_whittling`, `consume_light`, `consume_abyssal_echoes`, `consume_prevent_no_change`, `consume_sanctification`, `consume_blessed`, `consume_catalysing`, `consume_lord_target`, `consume_homogenising`.

## Errors (`crate::error`)

`EngineError` covers every refusal path with diagnostic detail:

- `InvalidApplication(String)` — generic refusal (rarity wrong, item corrupted, etc.)
- `ItemCorrupted` / `ItemSanctified`
- `AffixSlotFull { affix_type }`
- `NoEligibleMods { base, ilvl, affix_type }`
- `ModGroupExclusive(String)`
- `InsufficientMods { required, actual }`
- `FractureHiddenMod`
- `FracturedModImmutable(String)`
- `OmenIncompatible { omen, currency }`
- `PatchMismatch { required, running }`
- `Data(String)`
- `Other(String)`

The advisor surfaces these to the user as "this currency cannot be applied because …".
