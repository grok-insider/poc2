# Changelog

All notable changes to Path of Crafting 2.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — crafting-mechanics fidelity + PoE2 0.5

## [2.0.0](https://github.com/grok-insider/poc2/releases/tag/v2.0.0) - 2026-07-19

### Added

- *(advisor)* analytic transition-model builder replaces Monte Carlo training
- [**breaking**] migrate to Next.js/WASM web app + Electron desktop + PoE2 0.5
- *(v3)* Layer 2 — M15.1 strategies, M15.2 predicates, M15.3 rules, M15.4 cross-source CI
- *(v3)* Layer 1 data-substrate fixes + Layer 3 training infrastructure
- *(v2)* crafter helper v2 — Phases A-G + IPC/UI follow-ups
- *(engine)* wire Catalysts into the currency resolver
- *(engine)* M2.5 — Catalysts + Recombinator
- *(engine,advisor)* M2.9 — performance benchmark harness
- *(advisor,engine)* M4 — beam-search optimal-path advisor
- *(strategies)* M3 — strategy DSL + canonical worked-example fixture
- *(engine,pipeline)* M2.7 — concept-based mod analyzer (hybrid classification)
- *(engine)* M2.5 — Essences (Lesser/Normal/Greater/Perfect/Corrupted)
- *(engine)* M2.6 — Omen system + integration with Exalt/Annul/Chaos/Bone
- *(engine)* M2.5 — Hinekora's Lock + apply/preview/commit orchestration
- *(engine)* M2.5 — Bones + Well-of-Souls reveal
- *(engine)* M2.5 — Fracturing Orb (the user's 'checkpoint' mechanic)
- *(engine)* M2.4e — Greater + Perfect variants of Transmute/Aug/Regal/Exalt/Chaos
- *(engine)* M2.4d — Divine Orb + Vaal Orb
- *(engine)* M2.4c — Alchemy / Exalt / Chaos / Annul
- *(engine)* M2.4a+b — ModRegistry + Currency trait + Transmute/Augment/Regal
- *(engine)* M2.1 — domain types (Item, BaseType, ModDefinition, ids, tags)

### Other

- *(engine)* integration test for the user's Triple T1 ES body armour craft
- M1 foundation — flake, workspace, Tauri/Svelte skeleton, CI, docs

### Fixed — poe2db cross-validation pass (2026-06-11, follows the audit below)

A 31-class validation of the data bundle against poe2db.tw (18 classes via
parallel page validators, 13 via deterministic family-level pool diffs;
9 weapon classes now match poe2db's craftable pool exactly, armour deltas
explained by attribute-variant gating) plus 8 mechanics research passes.
Confirmed correct: all 132 Verisium Alloy class-targets row-for-row, all 45
omen texts word-for-word, alloys being Rare-only (uniques are verisium-
enhanced via the separate Runeforging mechanic, NOT alloys), essence tier
mechanics, and Gnawed/Ancient bone gates. Fixed:

- **Desecrated pools were fabricated**: replaced the 47 invented "of the
  Abyssal Lord" fixture mods with the real 0.5 pools — 240 entries (196
  equipment, exact match with poe2db's "Desecrated Mods /196": ilvl 65,
  Amanamu's/Kurgal's/Ulaman's prefixes + of-X suffixes, armour suffix-only;
  44 jewel "Lightless" mods at ilvl 1). Weapons/Quiver/Focus/Shield/Buckler
  pools now exist; swords/axes/claws/daggers/flails/sceptres genuinely have
  none in 0.5 (their pool is the unmodeled "Thrud's Might" mechanic) and
  bones now reject on them instead of stranding unrevealable slots.
- **Vaal implicits were fabricated**: deleted the 13 VaalImplicit_* fixture
  mods; the genuine per-class corrupted pools (RePoE `Corruption*` mods,
  38/38 verified vs poe2db for Belt/Ring/BodyArmour) carry
  `affix = Enchantment`, which the Vaal AddEnchantment sampler now draws
  from (it only read the Implicit index before — the real pool was
  unreachable, which is why the fixtures existed).
- **CoE essence join keyed on the wrong table**: essence tier maps use CoE
  `bases` ids, but the normalizer consulted `bgroups` first — overlapping
  low ids mislabeled every essence target (base 3=Belt read as "Boots",
  12/13=Dagger/One Hand Sword read as "Tablets"/"Charms"). Join order
  fixed; "Shield (DEX)"/"Shield (INT)" map to Buckler/Focus (separate PoE2
  classes). The six corrupted essences (Abyss/Delirium/Horror/Hysteria/
  Insanity/Breach) classify as remove-add-on-Rare by name — CoE exports
  them `corrupt=0` and they previously got Magic-promotion semantics.
- **Catalysts modeled PoE1, not 0.5**: removed the nonexistent Intrinsic/
  Unstable presets (rules now recommend Sibilant for caster), fixed
  Adaptive (the fixed attribute catalyst, +5 like all others — not a
  wildcard +10), gated base catalysts to Ring/Amulet (belts cannot be
  catalysed in 0.5), and added the 12 jewel-only `Refined` variants.
  `with_catalysts` now merges bundle entries over the presets instead of
  replacing them (the replace dropped every Refined variant in production).
- **Otherworldly mods wired up**: the 36 `GenesisTree*Crafted` mods are
  poe2db's per-class "Otherworldly" sections (4 amulet / 16 ring / 16
  belt), granted only by the previously-unmodeled **Altered Collarbone**
  breach desecration. Added `BoneSize::Altered` (Collarbone-only),
  `ModFlags::OTHERWORLDLY`, reveal-pool gating (regular bones never
  surface them), and reclassified the mods (kind=Desecrated + flag +
  classes). Bone matrix fixed: Cranium exists only as Preserved; Rib
  covers off-hand armour (Shield/Buckler/Focus); Jawbone covers
  Warstaff/Flail; full items desecrate by removing a random mod
  (poe2db), generalized to pool-bearing sides (armour pools are
  suffix-only, so 3-suffix armour frees a suffix).
- **Runemastered bases are uniques**: 241+ "VerisiumUnique" bases (the
  Runeforging upgrade targets) shipped as `released` and leaked into
  craftable base lists; the pipeline now forces them to
  `ReleaseState::Unique`.
- **Alloy mods lacked affix/level data**: RePoE exports the 50 `Alloy*`
  mods with empty generation types (they were silently dropped!); a
  curated 47-entry fixup table from poe2db assigns Prefix/Suffix +
  required levels, and the normalizer keeps them.

Known gaps recorded for a future pass: mod pools for Waystones (109 rows
on poe2db), Precursor Tablets (83), Relics (139), Life/Mana Flasks (57),
Charms (51), Inscribed Ultimatum (31), Expedition Logbooks (21); the
"Thrud's Might" weapon mechanic; Mark of the Abyssal Lord desecration
flow; Genesis-born per-class pool eligibility (birth simulation stays out
of scope); Preserved Vertebrae (waystone desecration); Vaal Catalysing
Infuser; Breach Ring quality caps (40/45); the 5 Expedition Saga omens;
Essence of the Abyss granting only one of its two Mark mods per class.

### Fixed — full crafting-surface audit (2026-06-11)

A deterministic matrix audit (new `audit-matrix` pipeline bin: 31 item
classes × base tiers × 8 item levels × all 33+ currencies × advisor
legality, ~1,600 checks against the live bundle) plus a poe2db.tw
cross-validation pass surfaced and fixed:

- **Engine: the apply orchestrator never enforced `can_apply_to`** —
  currencies whose restrictions live only in the pre-flight gate (e.g.
  Hinekora's Lock rarity set) applied to Unique items via
  `apply_currency_with_bases`. The orchestrator now runs the same gate
  the UI's `checkCanApply` consults (regression test in
  `rarity_gating.rs`).
- **Engine: essences had no item-class targeting** — any essence applied
  to any class (weapon essences landed on amulets across all 31 audited
  classes), and the single `target_mod` lost the per-class variants the
  data carries (1H vs 2H phys, attribute-pool defence splits). `Essence`
  now carries `class_targets` (mirroring `Alloy`), resolved per class +
  attribute pool at apply time; `essence_catalogue()` parses the CoE
  tier-group labels (exact classes, plurals, attribute splits,
  element-flavoured wand/staff families, `Jewellery`/`Offhands`/
  `One-Handed Weapons` aggregates) and intersects aggregates with each
  mod's `allowed_item_classes`.
- **Pipeline: 118 natural mod tiers were wrongly flagged `ESSENCE_ONLY`**
  — `flag_essence_target_mods` force-flagged every essence-granted mod,
  removing shared natural tiers (attack/cast speed, accuracy, damage,
  resists…) from every Transmute/Aug/Regal/Exalt/Chaos pool. This also
  made the Min-Modifier-Level keep-≥1-tier exception fall back to T1
  (req 1) instead of the strongest legal sub-floor tier on bows. The pass
  now flags only mods with **no positive natural spawn weight** (the 0.5
  bundle keeps 92 genuinely essence-exclusive mods flagged; probe test
  `floor_exception_probe.rs` pins the corrected behaviour).
- **Advisor: illegal alloy/emotion recommendations** — base-targeted
  Distilled Emotions were proposed on non-jewel bases (engine rejected
  them at apply), and alloys whose granted mod-group already occupied the
  item were proposed despite the engine's atomic remove-then-add
  rejecting that state. Candidate generation now mirrors the engine's
  base-name matching (via `PlanInput.base_registry`) and skips
  occupied-group alloys (regression tests in `candidate.rs`).
- New `audit-matrix` bin (`cargo run -p poc2-pipeline --bin audit-matrix`)
  reusable as a release gate; pipeline crate sets
  `default-run = "poc2-pipeline"`.

Known data gaps (tracked for the poe2db integration pass): no desecrated
mods for weapon classes or jewels in the bundle (bones for those classes
have no reveal options), and no essences target Traps/Quivers/Fishing
Rods.

### Added — desktop shell, item capture, price checking, cross-platform (ADR-0010, 2026-06-10)

Scope change by explicit user decision: Linux + NixOS + **Windows 11** are
supported targets; macOS stays out. ADR-0010 supersedes ADR-0002
(platform) and amends ADR-0001 (Electron). Preceded by a 21-agent
readiness audit (verdict: architecture sound; confirmed fixes below).

- **Electron desktop app** (`apps/desktop`): a normal windowed app (like
  Discord, NOT an overlay) around the unmodified web static export,
  served over a privileged `app://` scheme so the export's root-absolute
  asset URLs work without a server (WASM engine, worker, fonts, icons all
  verified live). Preload exposes exactly one bridge
  (`window.poc2Desktop`, contract in `apps/web/lib/desktop.ts`);
  single-instance with `--capture` flag forwarding; window-state
  persistence; external links open in the system browser. Dev on NixOS
  uses nixpkgs electron (portable launcher picks PATH electron on Linux,
  npm binary elsewhere); packaging via electron-builder (AppImage/deb +
  Windows NSIS) runs in CI on FHS runners.
- **Item capture** (Awakened-PoE-Trade semantics, verified against its
  source in `example-repos/`): hotkey → inject the game's own Ctrl+C →
  poll clipboard for `Item Class:` text (any client language) → push to
  the UI → restore clipboard after 120ms. Injection backends per
  platform: Windows `uiohook-napi` (optionalDependency, lazy);
  Linux/Hyprland `hyprctl dispatch sendshortcut` → `ydotool` → `wtype`
  spawns. Hotkey via `globalShortcut` (+ GlobalShortcuts portal on
  Wayland) with a compositor-bind fallback (`poc2-desktop --capture`).
  Renderer side: `ingestExternalItemText` store seam, wired in
  `bootDesktop.ts`. E2E-verified: main-process push → parse → bench.
- **Price checking**: new pipeline bin `fetch-trade-stats` generates
  `apps/web/public/trade-stats.json` (1,932 mod-text→trade-stat-id
  entries; vendored MIT Exiled-Exchange-2 dataset, `--live` merge from
  the official `/api/trade2/data/stats`); `lib/trade/statIndex.ts`
  matches the imported item's raw lines (EE2 matcher semantics);
  `lib/trade/queryBuilder.ts` builds real trade2 queries (min =
  rolled×0.9, bucket priority, ilvl toggle, unique names); new **Price
  Check panel** with toggleable stat rows, editable bounds, live search
  through the Electron main-process proxy (header-driven rate limiting,
  no CORS), grouped listings with cheapest/median/total, and an
  unknown-base degradation to stats-only search; browsers get the same
  query as a trade-site deep link. Live-verified against the real API
  (8,699-result search, listings rendered). Settings "Refresh prices"
  now assembles the real poe2scout snapshot and feeds the engine's new
  `applyPrices` (live prices reach the planner's valuator at last).
- **Cross-platform**: `XDG→HOME→APPDATA/USERPROFILE` path chains
  (market cache, pipeline tools), `rust-toolchain.toml` ships wasm32
  (no hardcoded host triple), `.gitattributes` enforces LF, the WASM
  build is a cross-platform Bun script (`scripts/build-wasm.mjs`; bash
  wrapper kept), CI gains a no-Nix `windows-latest` lane (cargo test +
  wasm + web + desktop tests + NSIS package) and a Linux packaging lane.
  Flake devshell gains `electron`.

### Fixed — foundation fixes from the 2026-06 readiness audit (all confirmed by adversarial verification)

- **Parser: plain-Ctrl+C items resolve mods correctly now.** Basic-format
  lines carry their numeric rolls and pick the tier whose stat ranges
  contain them, and `templates_match` understands the real bundle's
  template dialects — `(10-19)` ranges and `[EnergyShield|Energy Shield]`
  bracket markup — so captured rares land their modifiers on the bench
  (previously every such line was "not recognised as a modifier").
- **`Currency::apply` is atomic on failure**: `apply_currency_with_bases`
  snapshots and restores the item alongside the omen snapshot (essence/
  alloy/chaos error paths previously left the item mutated).
- **Item-class resolution placeholder removed**: `BaseRegistry::
  resolve_item_class` is the single path (4 call sites in advisor/
  strategies threaded through `PredicateContext`; engine fallbacks
  deduplicated; unresolved bases warn instead of silently misclassifying
  captured items).
- **`currency/basic.rs` god module split** (2,744 → 1,304 lines):
  shared sampling kernel → `currency/common.rs`, Greater/Perfect macro
  variants → `variants.rs`, Vaal model → `vaal.rs`; duplicated weighted-
  sampling loop and class fallbacks collapsed to one helper each.
- **Boundary divergences in `outcome.rs`**: slot capacity now comes from
  the engine's `Rarity` capacities (was hardcoded 3/3 — wrong for Magic
  items), ReplaceMod validates ilvl/group/slot, fractured mods can't be
  removed.
- **Live prices plumbing**: poe2scout snapshot application no longer
  requires the `net` feature; omen slug mismatches fixed
  (`OmenOfTheBlackblooded`, `OmenOfAbyssalEchoes`) with an
  exhaustive id-validation test; WASM `Engine.applyPrices` +
  typed client added.
- **Web**: persistence is a single debounced store subscriber (was 12
  manual `persist()` calls); captured imports surface the parse preview
  and unresolved lines (store-held `lastUnresolved`); mod display names
  no longer show the trailing group ordinal (`IncreasedLife7` →
  "Increased Life" — the digit is not the display tier); bun test runner
  with 52 tests (`lib/__tests__/`).
- **Docs/governance**: AGENTS.md rewritten for the new scope (+
  Architecture Conventions from the audit), ADR-0010 added, ADR-0001/
  0002/0009 amended, roadmap M10, `example-repos` registry updated
  (awakened-poe-trade).


Post-v1 work, layered on top of the v2 (`docs/80`) and v3 (`docs/81`)
iterations that already shipped on `main`. See
[`docs/83-crafting-fidelity-plan.md`](docs/83-crafting-fidelity-plan.md).

### Added — overlay-style item capture for the browser app (hotkey → hovered item → advisor) — ADR-0011

- **`crates/capture` (`poc2-capture`)** — the Hyprland capture daemon.
  Mechanism per the Awakened-PoE-Trade study (overlays don't OCR; they
  inject the game's own Ctrl+C): a compositor bind (`CTRL+SHIFT+D`,
  `+A` = advanced mods, `+S` = screenshot-OCR) runs `poc2-capture
  trigger` → the loopback daemon (`127.0.0.1:17771`) snapshots the
  clipboard, injects Ctrl+C via `hyprctl dispatch sendshortcut`
  (`ydotool` uinput fallback for raw-input games), polls `wl-paste`
  (50 ms × 12, localized `Item Class:` detection), restores the user's
  clipboard after 120 ms, and broadcasts the item over `WS /ws`
  (strict localhost/`app://` origin gate). OCR mode `grim`s a 560×360
  region around `hyprctl cursorpos` and broadcasts the PNG. Env-gated
  `?mode=test` fixture endpoint for E2E. Tokio + axum; loopback only.
- **Web bridge** (`apps/web/lib/captureBridge.ts`): silent auto-reconnect
  WebSocket client started at boot; `item-text` flows through the
  `ingestExternalItemText` seam (immediate import + jump to Item, undo
  intact), `item-image` flows through the tesseract.js OCR path. Status
  surfaces as a topbar `● capture` chip and a Settings → Capture card
  (daemon version, hotkeys, last capture, last error). Browser-only
  users are unaffected — the bridge fails silently.
- **Hyprland recipes**: `examples/hyprland/poc2-capture.conf` (binds +
  `exec-once`) + README setup (incl. `programs.ydotool.enable` on NixOS).
- **ADR-0011**: browser-side capture via compositor bind + loopback
  daemon, complementing ADR-0010's Electron shell (which embeds the same
  hyprctl → ydotool injection ladder in-process). Runtime research
  recorded: **Electrobun rejected** (X11-only hotkeys, FHS-assuming
  packaging hostile to NixOS, WebKitGTK default renderer); Electron's
  Wayland `globalShortcut` portal path noted as flag-gated and fragile —
  compositor binds remain the reliable hotkey mechanism on Hyprland
  for both transports.

### Fixed/Added — import pipeline: Grants-Skill lines, full parse preview, all-base icons, screenshot OCR

- **Parser**: `Grants Skill:` (and `Grants:`/`Charm Slots:`/`Duration:`/
  `Charges:`) lines are item properties, not modifiers — an Effigial Tower
  Shield no longer reports `Grants Skill: Raise Shield` as an unresolved
  mod (regression test in `crates/parser/src/text.rs`).
- **Full parse preview.** Importing now renders the whole parsed item as a
  PoE2 item popup (`.poe-pop`): rarity header sprite, base icon, class /
  ilvl / quality line, implicits + P/S-tagged mod lines in magic blue with
  rolled values, fractured/crafted coloring, corruption flags, and every
  unresolved line listed in red — nothing is hidden behind a chip anymore.
- **Icons for ALL bases.** `fetch-base-icons` v2 scrapes the ~30 poe2db
  class-listing pages instead of 3,800 detail pages: each row's
  `data-hover` carries the GGPK metadata id — an exact join key onto bundle
  `BaseTypeId`s (no name fuzzing). 1,770 released gear bases now resolve
  icons (leveling bases included; 0 duplicate conflicts, dedupe-validated;
  the previous drop_level>50 filter is gone). Manifest fetches use
  `no-cache` so re-runs show up without a hard reload.
- **Target card redesigned** (`TargetSummary`): structured spec rows
  (count × concept(s) in mod-blue · tier badge · P/S marker, click-to-edit),
  labeled min/expected/max budget with meter, and risk/depth sliders with
  qualitative captions ("balanced", "lookahead") per the UX pass.
- **Screenshot OCR import** (`apps/web/lib/ocr.ts`). Research first
  (Awakened PoE Trade + Exiled Exchange 2 cloned and read): desktop
  overlays capture items via simulated **Ctrl+C → clipboard**, not OCR —
  Ctrl+C stays our lossless path. The new OCR covers screenshots (consoles,
  cropped images): paste an image anywhere in the Import panel (or pick a
  file) → canvas preprocess (3× upscale, max-channel luminance threshold,
  invert) → lazy-loaded tesseract.js (PSM 6, char whitelist) → fuzzy
  base-name match against the icon manifest (bigram similarity — the
  dictionary-correction trick from the studied apps) → reconstructed
  clipboard text through the normal parser, with explicit ilvl-floor
  warnings and all leftovers surfaced in the parse preview for correction.

### Changed — PoE2 in-game design system (whole app) + Genesis 1:1 pass

- **The web app now reads like a native PoE2 panel.** New
  `apps/web/DESIGN.md` defines the system (researched from in-game
  screenshots + poe2db's GGPK-mirrored assets/`stdtheme.css`): GGG's
  **Fontin SmallCaps / Fontin Regular** webfonts (fetched from
  `web.poecdn.com` into `public/fonts/`), black panels with bronze
  hairlines, ONE gold accent system (`--gold` `#b29155` / `--gold-bright`
  `#e7b478` / `--gold-action` `#d29933` — the blue accent is gone),
  square corners, metal-bevel buttons, gold-underline section headers,
  a `.poe-plaque` title bar, the game-exact rarity palette
  (`#8888ff` magic, `#ffff77` rare, `#ef6916` unique, `#aa9e82` currency,
  `#b4b4ff` crafted, `#d20000` corrupted), and a 1:1 `.poe-pop` item-popup
  family driven by the fetched `popup2` header/separator sprites.
  Implemented as a token/primitives retheme of `globals.css`, so every
  panel inherits it.
- **Genesis Tree is now a 1:1 in-game recreation.** The tree renders with
  the REAL BreachLeague node-frame sprites named by `BrequelTree.json`'s
  `art` section (small/notable × normal/can-allocate/active, womb slot,
  node glow — all fetched by `fetch-genesis-assets`), in-game purple link
  colors, and Breach-style node tooltips (dark-red header band, cream
  title, magic-blue mod lines, gold action hints). The page is full-bleed
  (no bench column) with the layout `farming notes | tree (dominant) |
  goal presets`.
- **Presets are now graph-resolved and budget-verified.** The pipeline
  resolves every preset against the real tree graph (BFS from the womb
  root): steps carry exact `node_ids`, forced `connector_ids` ("pathing
  nodes") and cumulative `points_after`; presets carry
  `core_points`/`points_cap`. A pipeline test fails the build if a
  preset's core path exceeds its womb cap — which caught and fixed the
  original data (minion-belt was 19/10, caster-ring 13/10, attr-amulet
  16/15; over-budget steps are now `optional` "respec options" and
  multi-copy fillers are `fill` steps). The UI highlights the full
  connected route (active frames + glow + lit edges), pathing nodes,
  dashed respec options and red ✕ avoid nodes.

### Added — Genesis Tree panel (0.5 "Return of the Ancients")

- **Full Genesis Tree ("Brequel") UI** (`apps/web/components/GenesisPanel.tsx`):
  renders all 248 datamined passives of the five Womb branches (Currency 15 /
  Amulet 15 / Ring 10 / Belt 10 points; Breachstone is slot-only) at their real
  in-game layout positions (group + orbit math from RePoE-fork's
  `BrequelTree.json`), with connection edges, PoE2-style node frames (gilded
  notables, desecrated-violet womb sockets), wheel-zoom/drag-pan, and
  item-tooltip-styled hovers. Node icons + Wombgift art are fetched by the new
  `fetch-genesis-assets` pipeline bin into `apps/web/public/genesis-icons/`
  (regenerable, gitignored).
- **Goal presets — "which nodes for which drops"**: seven curated, source-cited
  community allocations (Divine farming, Exalt fishing/SSF, Catalyst farming
  (measured ~1.5 div/map), minion belts, caster rings, attribute/resistance
  amulets, Breachstones) with per-node priorities + "why", avoid-lists, gift
  advice, Hiveblood farming notes and five vetted videos. Selecting a preset
  highlights its nodes on the tree (priority badges, avoid halos) and offers a
  copy-as-text node list. All soft numbers labeled community estimates.
- **Data path**: committed `pipeline/data/brequel_tree.json` snapshot + curated
  `pipeline/data/genesis_meta.toml` (109 stat-key templates, 37 display-node
  overrides, womb metadata, presets) → `normalize_genesis` →
  serde-defaulted `bundle.genesis` section → WASM `Engine.genesisTree()` →
  typed client (`GenesisTreeView`). No engine simulation — UI/advisor
  knowledge only, per scope decision.
- **Advisor guidance**: new `crates/rules/seed_rules/14_genesis_tree.toml`
  (R500-R503) — points Ring/Belt/Amulet crafts at the Genesis-exclusive
  caster/minion mod pools, the 14 tree-exclusive bases, max-suffix amulet
  births, and the 0.5 catalyst supply gate.

### Added — Verisium Alloys end-to-end + Distilled Emotions (0.5)

- **Alloys are now fully data-driven and advisable.** Curated
  `pipeline/data/alloys.json` binds all **13 alloys × 132 class-targets**
  (scraped from poe2db, joined to RePoE `Alloy*` mods by text/range) into
  `bundle.alloys` (v2 class-targeted shape). Engine `Alloy` gained
  `class_targets` (per-item-class granted mod, resolved at apply);
  `CurrencyResolver::alloys()` exposes the catalogue and the advisor's
  candidate generator proposes goal-relevant alloys (concept-gated, Rare-only,
  crafted-cap-aware) as deterministic finisher moves.
- **Liquid / Potent / Ancient Emotions** (jewel crafting): curated
  `pipeline/data/emotions.json` (26 emotions × 96 base-targets, all bound)
  → `bundle.emotions` → `emotion_catalogue()` seeding the resolver. Emotions
  reuse the alloy remove-then-add mechanic with **base-name targets**
  ("Ruby" / "Time-Lost Sapphire" / …; exact match — Ancient emotions do not
  collide with plain bases), sampling uniformly among same-base targets.
  The jewel mod pool itself (371 mods: `strjewel`/`dexjewel`/`intjewel` +
  radius + `CraftedJewel*` lines, domain `misc` upstream) is now ingested
  into the registry under `ModDomain::Jewel`.

### Changed — 0.5 mechanics fidelity (crafted/desecrated caps, multiply rolls)

- **`ModKind::Crafted`** — new mod kind for Alloy / Emotion / Genesis crafted
  outputs (parser maps `(crafted)`-tagged lines to it). 0.5's "items can only
  have 1 crafted modifier" is enforced by `Alloy` (`can_apply_to` + `apply`)
  via `Item::has_crafted_mod`; "limited to 1 Desecrated modifier" is enforced
  at bone application (patch-gated ≥0.5, `Item::desecrated_mod_count`).
- **Vaal corruption + Sanctification now multiply in 0.5.** The
  "unpredictable values" corruption outcome rerolls within range ≤0.4 but
  multiplies each modifier's current values by a per-mod uniform factor
  ([0.8, 1.25], Experimental) in 0.5+; Sanctification likewise multiplies
  ([0.8, 1.2]) instead of rolling the 80-120% extended range. The
  **Omen of Sanctification and Omen of the Blessed are now actually consumed
  by Divine Orb** (previously declared but never wired).
- Instant-Leech desecrated mods need no engine gate in 0.5 — they are already
  absent from the live RePoE 0.5 export (data-level removal confirmed).

### Added — League ruleset in the UI

- WASM `Engine.setLeague("standard" | "challenge")` (+ `league` getter)
  replaces the hardcoded `League::current()`; the worker/client expose it and
  the store persists `engineLeague` (IndexedDB), syncing the engine before
  the first plan and re-planning on change.
- Settings grew a **League ruleset** toggle (Runes of Aldur ↔ Standard) and
  the price-league presets now lead with **Runes of Aldur** (stale
  "Dawn of the Hunt" removed).

### Fixed — no premature Divine on partial items

- The `tier-fix-divine` / `tier-fix-fracture` heuristics
  (`crates/advisor/src/candidate.rs`) are value-**polish** steps, but nothing
  stopped them firing on an unfinished item — so a Magic body armour with one
  off-target mod was told to **Divine Orb** to "refine toward T1 max value"
  instead of building toward its goal. They're now gated on
  `goal::is_satisfied` and only fire once the item already carries all the
  target mods (the legitimate "max/lock the values" case). On a partial item
  the planner now recommends a building action (e.g. Perfect Augment) instead.

### Changed — anti-myopia planner scoring, honest P(reach goal), deeper lookahead

- **Goal-progress is now a first-class scoring signal** (`score_node`,
  `crates/advisor/src/planner.rs` + `ScoringWeights::progress_bonus`,
  `scorer.rs`). The score keeps the multiplicative reliability×progress
  attainment term (which zeroes out no-progress no-ops like a destructive
  Annul) and adds an explicit, weighted structural-progress reward (activates
  the previously-dead `progress_bonus`, default `1.0`) so that among building
  steps the one reaching *more* of the target wins even when it's riskier.
- **Honest headline number.** `Recommendation.expected_prob` was the raw
  joint step-execution probability — a safe-but-useless step read ~90%. It is
  now **P(reach goal) = execution-reliability × goal-progress of the current
  item**, and a new `goal_progress` field carries the structural
  fraction-of-specs the *current* item satisfies (stable, not the noisy
  single-rollout terminal state). The web (`GuidePanel.tsx`) relabels the
  headline **"P(reach goal)"**, adds an **"n/m specs"** progress bar, and
  re-tunes the risk colours (`format.ts`) for the goal-attainment scale.
- **Deeper default lookahead.** The web now plans at **depth 4** (was 2,
  `store.ts`): a magic→rare build needs ~4 steps to reach a goal-satisfying
  state, so shallower search pruned building paths before they earned progress.

### Added — build-archetype targets, base-item icons, monochrome redesign

- **`Spirit` + `SkillLevel` are now first-class concepts** (`crates/engine/src/concepts.rs`):
  a load-time pass derives them from each mod's group/text (additively), so the
  PoE2 minion-resource and `+Levels of … Skills` mods — previously lumped under
  `Other` — are now targetable and surfaced in the eligible-mods palette. This
  also makes the codified `*-spirit` strategies resolve.
- **Archetype-aware "Suggest"** (`apps/web/lib/archetypes.ts`): curated build
  presets per item class + attribute variant (e.g. int body armour → *Max ES
  (CI)* / *Minion (ES + Spirit)* / *Caster*; quarterstaff → *Phys attack* /
  *Elemental*). The Target panel shows a row of preset chips (`Match current
  mods` + the matching sets) that apply a full ~6-mod target in one click. Each
  preset is validated against the base's real eligible pool: concepts the base
  can't roll are dropped, each concept is placed in the affix the base actually
  rolls it on, and `min_tier` is clamped to the best tier reachable at the item
  level. So *Minion* on an int armour seeds `EnergyShield ×2 (no hybrid)` + a
  `Spirit` prefix + tri-resistance — the canonical ES-stacker minion target.
- **Base-item icons** (`pipeline/src/bin/fetch_base_icons.rs` + `apps/web`):
  the scraper now filters to drop level > 50 and writes a manifest +
  `<class>/<file>.webp` set (gitignored, regenerable). A new `<BaseIcon>`
  component renders them (letter-glyph fallback when absent) in the item card
  and database browser.
- **UI redesign — dark monochrome** (`apps/web/app/globals.css`): re-skinned
  the whole app to black/white/grey on the 60-30-10 rule — near-black canvas,
  graphite panels, near-white text, a single accent (`#7db4ff`) reserved for
  the primary action / active / focus, muted success/danger (always paired with
  a glyph/label), and desaturated rarity hues. The token sheet drives every
  component, so the re-skin propagates without structural change.

### Added — paste an item, pick a base- & ilvl-aware target

- **Advanced Mod Descriptions clipboard parsing** (`crates/parser`): the parser
  now auto-detects and parses PoE2's `{ <Affix> Modifier "<name>" (Tier: N) —
  tags }` headers with `value(min-max)` rolls, plus the `Requires: …` one-line
  and `Sockets:` formats. Annotated mods resolve to the exact `ModDefinition` by
  name + tier + affix (not fuzzy text), and `ModRoll.values` are populated from
  the parsed rolls.
- **Base & class resolution** (`crates/poc2-wasm`): the printed item class
  (incl. irregular plurals like `Foci`→`Focus`) resolves to the canonical id,
  and the base name (Magic affix words stripped via the annotation names)
  resolves to the **real bundle `BaseTypeId`** — so the engine applies the
  correct **attribute-variant (str/dex/int) modifier pool**. The parse response
  gained `base_display_name` / `item_class_id` / `base_resolved` / `warnings`.
- **`eligible` command now honours the caps** (base **and** item level): it
  resolves the class + tags from the `BaseRegistry` and weights via
  tag-intersection, so the eligible pool excludes mods the base's attribute
  variant can't roll (e.g. an int armour no longer lists Armour mods), while
  `eligible_now`/`required_level` reflect the ilvl gate. (Also fixed the
  `tier_index` off-by-one in the web tier display; tier 1 = best, matching the
  in-game annotation tier.)
- **Web: paste → target** (`apps/web`): pasting an item resolves the base and
  shows it (name · class · str/dex/int variant) with an "approx. pool" warning
  when unresolved. A new **"Suggest from item"** action seeds the target from
  the item's current mods at the **best tier reachable on this base at this
  ilvl**, and the Target panel's concept palette is now sourced from the base's
  real eligible pool (so an int Focus offers EnergyShield/caster concepts, not
  Armour/Evasion), each chip showing its best achievable tier. New
  `apps/web/lib/concepts.ts` + store `eligible`/`refreshEligible`/
  `seedTargetFromItem`.

### Changed — UI rebuilt as a WebAssembly web app

- **The Tauri 2 + Svelte 5 desktop app (`apps/desktop`) was replaced by a
  Next.js 16 + React 19 web app (`apps/web`)** that runs the Rust advisor
  **in the browser via WebAssembly**. The desktop webview was effectively
  unclickable under the target compositor; a real browser fixes that and
  removes all native/Wayland coupling. No server, no install — every
  recommendation is deterministic client-side compute.
- **New `crates/poc2-wasm`** (wasm-bindgen `cdylib`): an in-memory `Engine`
  exposing `recommend`, `parse`, `eligibleMods`, `checkCanApply`,
  `recordOutcome`, `rerollableMods`, `runNTrials`, `recoveryHints`,
  `listBases`, `listDatabaseEntries`, `databaseEntryDetail`. Hosted in a Web
  Worker so planning never blocks the UI. `scripts/build-wasm.sh` builds it
  (cargo wasm32 → wasm-bindgen `--target web` → `wasm-opt -Oz`, ~2 MB).
- **`crates/data::read_bundle_bytes`** decodes a gzip/JSON bundle from memory
  (the web app fetches `poc2.bundle.json.gz` as a static asset).
- **`crates/market` networking is now behind a `net` feature** (off by
  default at the workspace level) so the advisor stays WASM-clean; the
  `Valuator` type is always available. `crates/strategies::seed_strategies()`
  embeds the strategy TOMLs via `include_dir`.
- **Full feature parity** ("Forge" design, `docs/90-ui-redesign.md`): item
  editor + clipboard paste import, target editor, guide (hero recommendation
  + success band + alternatives + recovery hints), eligible-mods inspector,
  history with undo, database browser, simulation runner with distribution
  chart, recipe library, settings, and the outcome dialog
  (add/remove/reroll/rarity). State persists to IndexedDB (`idb-keyval`).
- **Dropped as desktop-only** (no browser equivalent): the Client.txt live
  watcher and the in-process Wasm plugin host. The Rust workspace gate stays
  green (`cargo fmt`/`clippy`/`test --workspace`); the flake drops the Tauri
  system deps and adds the wasm toolchain (`wasm-pack`, `wasm-bindgen-cli`,
  `binaryen`) + the `wasm32-unknown-unknown` target. The web app uses **Bun**
  as its package manager + script runner, set up as a **root Bun workspace**
  (root `package.json` + single root `bun.lock`) so `bun install` and
  `bun run <dev|build|typecheck|lint|wasm>` all run from the repo root. ESLint
  runs via a flat config (`next lint` was removed in Next 16).

### Added — crafting-mechanics fidelity (engine)

- **Inclusive higher-tier weighting** for normal currency sampling
  (Exalt / Aug / Regal / Chaos). A tier now inherits the spawn weight of
  the same-group higher tiers rollable at the current item level
  (`Σ_{j=m_i}^{m_t0} w_j`, `m_t0` ilvl-dependent) — matching in-game odds
  (`ModRegistry::inclusive_weight_for`).
- **Item-level-dependent pools**: raising ilvl unlocks higher tiers, so the
  rollable pool grows with ilvl (boundary-tested).
- **Keep-≥1-tier Minimum-Modifier-Level exception**: a Greater/Perfect
  floor excludes *tiers* but never an entire mod-type — if every tier of a
  group is below the floor, its highest tier still rolls.
- **Patch-versioned Min-Modifier-Level floors** (`MinModLevelVariant`):
  wiki-correct Greater Exalted = 35 / Perfect Exalted = 50; 0.5 lowers the
  Greater Transmute/Aug floors.
- **Explicit tier ordinals** on `ModDefinition` (pipeline `assign_tier_ordinals`
  post-pass; tier 1 = strongest).
- **Tag-intersection weighting** (leftmost-tag-wins) via
  `ModRegistry::weight_for_on_base` + `ModDefinition::spawn_weight_for_tags`,
  using `BaseRegistry` tags.
- **Desecration fidelity**: bone-size → item-level semantics (Gnawed ≤ ilvl
  64, Ancient guarantees Min Modifier Level 40, Preserved unrestricted);
  lord-targeting omens (Liege/Sovereign/Blackblooded) restricted to
  Weapons & Jewellery; lord omens brick the Ancient floor.
- **`League` ruleset** threaded through `ApplyContext` (Standard vs the
  current challenge league).

### Added — cross-version (0.3 / 0.4 / 0.5) gating

- `recombinator_available(patch, league)` + `recombine_gated`: the
  Recombinator is disabled in 0.5 Runes of Aldur (Standard-only). The
  advisor's candidate generator drops Recombine candidates when the
  Recombinator is unavailable for the active `(patch, league)` — `League`
  is now threaded through `PlanInput`.
- Omen of Corruption gated Standard-only in 0.5
  (`OmenSet::consume_prevent_no_change(patch, league)`).
- Homogenising omens remain 0.3-only.

### Added — new 0.5 crafting systems

- **Verisium Alloys** (`currency::Alloy`): essence-like "replace one
  modifier with a guaranteed crafted mod" on a Rare item, gated to 0.5+,
  with Crystallisation affix-forcing and family-collision rules. Resolved
  by `DefaultCurrencyResolver::with_alloys`.
- **Verisium Alloy catalogue wiring** (§5.3): the bundle now carries a
  serde-defaulted `alloys` section (`{ id, name, engine_mod_id }` entries)
  and `Bundle::alloy_catalogue()` extracts it into typed `Alloy`s,
  analogous to `essence_catalogue` / `catalyst_catalogue`. The desktop
  loader and `train-advisor` seed the resolver via `.with_alloys(...)`, so
  alloys resolve + apply end-to-end once a 0.5 bundle emits the section.
  `Alloy` is now re-exported at the crate root (`poc2_engine::Alloy`).
- Genesis Tree mods + 12 new Jewel catalysts are data-driven: Genesis Tree
  mods flow through tag-intersection weighting; the new jewel catalysts use
  the existing data-driven `Catalyst` (Jewel is already an eligible class).
  Both populate from the live 0.5 bundle (operator rebuild).

### Changed

- **Bundle schema → v3.** `ModDefinition.tier` + `HiddenDesecratedSlot.min_mod_level`
  are serde-defaulted but v2 bundles are hard-rejected (rebuild required).
- **Greater Transmute/Aug 0.5 floors confirmed from data.** Replaced the
  community-estimate `TODO(0.5-data)` values (20 / 35) with the 0.5.0
  patch-note value: Greater Orbs of Transmutation and Augmentation now have a
  Minimum Modifier Level of **44** (previously 55), confirmed against poe2db's
  per-currency field. `MinModLevelVariant::floor` now uses 55 pre-0.5 and 44 in
  0.5+ for both, and the TODO markers are removed.
- **Pipeline emits ilvl-stratified weights** (`WeightScope::BaseAtIlvl`, §5.5).
  The CoE→engine join now emits one `BaseAtIlvl` observation per real weight
  breakpoint of a base's tier ladder instead of collapsing to the top tier, so
  the engine resolves the correct spawn weight per item level (high-ilvl items
  resolve identically to before; lower item levels are now accurate). Uniform
  ladders still emit a single flat `Base` observation.

### Fixed — 0.5 bring-up

- **Advisor ranking:** an always-on `Guidance` rule (e.g. R304 bankroll
  advice) could surface as the top recommendation ahead of the concrete
  next step. Guidance actions are now kept out of beam expansion (they don't
  mutate the item) and only surface as a fallback when no concrete step
  exists; the rule's note now populates the guidance card (previously empty).
- **Trained-model version guard:** `load_artefact_file` now refuses trained
  models whose `bundle_schema_version` / `engine_schema_version` don't match
  the current build (e.g. a stale 0.4 schema-v2 model next to a 0.5 schema-v3
  bundle), so the advisor falls back to heuristic planning instead of
  consuming mis-keyed Q-values.
- **Lesser/Normal essence rarity gate.** `EssenceQuality::Lesser` / `Normal`
  declared `valid_rarities() == NORMAL`, but `apply_promoting` requires Magic —
  so their success path was unreachable (the rarity gate allowed only Normal
  items, which apply then rejected). Per the wiki/poe2db, Lesser/Normal/Greater
  all "upgrade a Magic item to a Rare item, adding a guaranteed modifier", so
  all three now correctly gate to **Magic**. Fixes the advisor offering these
  essences on the wrong rarity and makes them actually usable.
- **Remove-then-add affix overflow:** Perfect/Corrupted Essences and Verisium
  Alloys remove one modifier then add their guaranteed mod. When the crafted
  mod's affix side was already full (3) and the removal targeted the *other*
  side (a random draw, or Sinistral/Dextral Crystallisation forcing it), the
  add could overflow to an illegal 4th prefix/suffix. The removal is now
  constrained to free a slot on the crafted mod's side; a contradictory
  Crystallisation (forcing the opposite side while the target side is full)
  returns `AffixSlotFull` instead of corrupting the item.

### Data / models

- **CoE alias curation (§5.1).** Added 17 verified general-explicit aliases to
  `pipeline/data/coe_aliases.toml` (flat phys, all-attributes, ailment
  durations, stun/crit/pierce, thorns, and the Sceptre "Allies in your
  Presence" presence mods). Each was individually checked against the live
  bundle's RePoE-fork mod (kind=Explicit, no essence/corrupted/desecrated-only
  flag, template matches); wrong-domain auto-suggestions were rejected. The
  alias suggester confirms 17 fewer unmatched CoE mods (433 → 416).
- **0.5 advisor model retrained (§5.4).** A schema-v3 trained-model artefact
  (51 goal models) was produced from the live 0.5 bundle and installed at
  `~/.config/poc2/cache/trained_models/poc2-trained-models-0.5.0.json`; the
  version guard accepts it (bundle_schema=3 / engine_schema=1) while the stale
  0.4 schema-v2 artefact is correctly ignored. (Smoke-level `--samples 1000`;
  production uses `--samples 100000`.)

### Verified

- A live **0.5.0 bundle** builds from RePoE-fork + CoE + poe2db (schema v3,
  3098 mods, 4988 weights, tier ordinals assigned) and the advisor plans
  against it end-to-end (`live_bundle_smoke` test). Full workspace builds
  (incl. desktop Tauri crate + `pnpm build`); fmt/clippy/test/`pnpm check`
  all clean.

### Docs

- New `docs/14-crafting-mechanics-cross-version.md` (0.3/0.4/0.5 delta
  matrix) and `docs/83-crafting-fidelity-plan.md`; refreshed
  `docs/11-game-mechanics.md` to the 0.5 baseline.

### Tests

- New: `weight_ilvl_pool`, `min_mod_level_pools`, `crafting_invariants_proptest`,
  `desecration_mechanics`, `desecration_gating`, `essence_mechanics`,
  `cross_version_gating`, `recombinator_league_gating`.
- Expanded edge-case coverage (~48 added tests; full suite now 651): essence
  & alloy remove-add overflow regressions + Crystallisation contradictions,
  Sinistral/Dextral forcing, family-collision, missing-target, sanctified/
  corrupted/mirrored gates; the full 10-arm `MinModLevelVariant` floor matrix
  across 0.4/0.5 and Greater/Perfect Aug/Transmute/Chaos floor enforcement;
  bone subtype→class gating (Collarbone/Cranium/Jawbone), lord-omen scope +
  single-lord consumption; Vaal Omen-of-Corruption league gating (0.5
  Standard vs Challenge), AddQuality cap, brick-preserves-fractured;
  `spawn_weight_for_tags` / `tier_strength_key` / inclusive tag-intersection
  weighting; Alloy bundle round-trip + `alloy_catalogue` extraction + resolver
  seeding; trained-model engine-schema-version guard; tier-ordinal tie-break +
  single-mod group.

## [1.0.0] — 2026-04 (release)

First public release. NixOS + Hyprland only per ADR-0002 + ADR-0009.

### Added — engine + data

- **Engine core** with sub-µs `apply_currency`. 15 currencies +
  Greater/Perfect tiers, 22 omens, fracturing orb, Hinekora's Lock
  (preview/commit byte-equality), bones + reveal, catalysts,
  recombinator, full hybrid-mod handling.
- **Data pipeline** (poc2-pipeline) producing 211 KB gzipped bundles
  from RePoE-fork + Craft of Exile + poe2db.tw. ≥80% CoE→engine
  mod-id join via the four-tier strategy (alias / essence-xref /
  name-substring / template-tokens) — see Phase A.3.
- **23 codified strategies** (full coverage of /docs/33), each
  shipping as TOML with preconditions, target spec, step graph,
  abandon criteria, and recovery hints.
- **113 production rules** across 14 sections (full coverage of
  /docs/34), TOML-driven via `crates/rules/seed_rules/`.

### Added — advisor

- **Beam-search optimal-path planner** with configurable
  `(width, depth, top_n, risk, mc_samples)` (Phase M4).
- **Monte Carlo aggregator** with `prob_stderr` confidence bands
  (Phase C.1; default 50 samples per candidate; depth-3 perf 139 µs
  vs 5 ms budget).
- **Streaming recommendations** at depth 1 → 3 → final via
  Tokio + Tauri events; cancellable on new requests (Phase C.2).
- **PredicateContext** threading: cost-aware, stash-aware,
  valuator-aware, sale-price-aware, plugin-aware predicates fire
  mid-plan (Phases A.1 + F.3).

### Added — UI (Tauri 2 + Svelte 5)

- **Item builder + clipboard import** (M6 / M7).
- **Target panel** (Phase B.1) with concept picker, hybrid toggle,
  budget triple editor; persists to `~/.config/poc2/state.toml`.
- **Recovery panel** (Phase B.2) surfacing strategy-step recovery
  hints when `lastFailed=true`.
- **Settings panel** (Phase B.3) with bundle hot-swap, league
  dropdown (poe2scout `/Leagues`), prices auto-refresh, Client.txt
  watcher, plugin manager, off-meta crafting hints.
- **Recipe library** (Phase B.4) save/load/share via
  `~/.config/poc2/recipes/<name>.toml`.
- **Simulation runner** (Phase C.3) with inline-SVG histogram of
  the change-count distribution + per-trial cost.

### Added — live integration

- **Client.txt watcher** (Phase D.1) via `notify` crate;
  area / player / death / whisper events emitted on
  `client-log://event`.
- **Hyprland always-on-top** (Phase D.2 / ADR-0009) via
  windowrulev2 recipes; example configs in `examples/hyprland/`.
- **Trade URL search** (Phase D.3) builds
  `pathofexile.com/trade2/search/...` deep-links from the current
  item state and opens via `tauri-plugin-shell`.

### Added — market

- **poe2scout live price feed** (M5.3) with conservative defaults +
  per-currency band overrides + UI refresh button.
- **poe.ninja meta-build aggregator** (Phase E.1) with permissive
  deserializer; soft-fails on endpoint absence.
- **Off-meta niche finder** (Phase E.2) ranks concepts by
  `demand_share / sqrt(competition + 1)`; surfaced in Settings as a
  "What to craft right now" card.

### Added — Wasm Plugin SDK (Phase F)

- **`poc2-plugin-host` crate** with wasmtime engine + capability
  gating + per-plugin sandboxing (fuel budget) + predicate dispatch
  cache (4096-entry LRU).
- **`poc2-plugin-sdk` crate** with `declare_predicate!`,
  `declare_strategies!`, `declare_rules!`,
  `declare_recommendation_emitter!` macros.
- **7 capabilities** (read_engine / read_market / read_advisor_state /
  register_predicate / emit_strategies / emit_rules /
  emit_recommendations) declared per plugin in `poc2-plugin.toml`.
- **`ItemPredicate::Custom`** variant referenceable from any rule
  or strategy TOML; dispatch routed through the host's
  `PluginPredicateDispatch` trait.
- **Example plugin** (`examples/plugins/predicate-ilvl-min/`)
  demonstrates the `declare_predicate!` macro end-to-end.
- **Plugin Manager UI** lists every loaded plugin with id, version,
  capabilities, strategy/rule counts; reload-from-disk button.

### Added — documentation

- 9 ADRs (`docs/adr/0001` through `docs/adr/0009`).
- 14 architecture / mechanics / strategy / rules / UI / market /
  recovery / probability / decision-engine docs.
- `examples/hyprland/` + `examples/plugins/` with working
  configs / source.

### Performance (verified by `cargo bench --bench advisor_plan`)

| Operation | Time | Budget | Margin |
|---|---|---|---|
| `plan_depth_1_top_3` | 46 µs | 1 ms | ×21 |
| `plan_depth_3_top_3` | 46 µs | 50 ms | ×1086 |
| `plan_depth_3_top_3_mc50` | 139 µs | 5 ms | ×35 |
| `plan_depth_5_width_8` | 151 µs | 500 ms | ×3311 |

### Removed

- Phase G.1's planned beam-search memoization (was: canonicalize
  Item by tier-set). Measured numbers showed no need at v1 scale;
  deferred to v1.x as an "if needed" optimization.

### Tests

- 317 workspace tests passing across 11 crates + the desktop app.
- Canonical rediscovery test (`crates/advisor/tests/canonical_rediscovery.rs`)
  asserts the advisor's top-N includes Perfect Transmute traceable
  to rule R001 or strategy `3xt1-es-body-armour-isolation` step S2.
- 11 plugin-host integration tests verifying load + dispatch +
  cache + capability-gate + perf budget.
