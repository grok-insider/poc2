//! High-level pipeline orchestration: fetch all sources, normalize, validate, return a Bundle.

use poc2_data::Bundle;
use poc2_engine::PatchVersion;
use tracing::{info, warn};

use crate::error::PipelineResult;
use crate::http::make_client;
use crate::normalize::{
    assign_tier_ordinals, flag_essence_target_mods, normalize_coe, normalize_fixtures,
    normalize_genesis, normalize_poe2db, normalize_repoe,
};
use crate::sources::{coe, fixtures, genesis, poe2db, repoe};

/// Pipeline build options.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub game_patch: PatchVersion,
    pub built_by: String,
    /// Whether to skip the cross-reference validation step (useful when
    /// upstream is partially populated; defaults to `false`).
    pub skip_validation: bool,
    /// Skip Craft of Exile fetch (offline mode / faster pipeline runs).
    pub skip_coe: bool,
    /// Skip poe2db scrape (offline mode).
    pub skip_poe2db: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            game_patch: PatchVersion::PATCH_0_5_0,
            built_by: format!("poc2-pipeline@{}", env!("CARGO_PKG_VERSION")),
            skip_validation: false,
            skip_coe: false,
            skip_poe2db: false,
        }
    }
}

/// Build a complete bundle by pulling from every source.
///
/// Sources are fetched in order, with optional sources (CoE, poe2db)
/// failing soft — a network error from CoE doesn't kill the build, it
/// just leaves the corresponding bundle sections empty (with a warning).
pub async fn build_bundle(opts: BuildOptions) -> PipelineResult<Bundle> {
    info!(?opts.game_patch, "starting bundle build");
    let client = make_client();

    let mut bundle = Bundle::empty(opts.game_patch, opts.built_by);

    // ---- Live PoE2 patch pointer (provenance only, best-effort) ---------
    // Stamp the community patch-version pointer into the bundle header so any
    // built bundle is traceable to the live game version it corresponds to
    // (ADR-0012). Soft-fail: a missing pointer never blocks a build.
    if let Some(patch) = crate::watch::fetch_live_patch(&client).await {
        info!(live_poe2_patch = %patch, "stamped live PoE2 patch pointer");
        bundle.header.sources.0.push(poc2_data::SourceRevision {
            name: "poe2.patch_pointer".into(),
            revision: patch,
            url: Some(crate::watch::POE2_PATCH_POINTER_URLS[0].into()),
            fetched_at: iso_now(),
        });
    } else {
        warn!("PoE2 patch pointer unreachable; bundle will not carry a live-version stamp");
    }

    // ---- RePoE-fork (mandatory) -----------------------------------------
    info!("fetching RePoE-fork…");
    let snapshot = repoe::fetch(&client).await?;
    info!("{}", snapshot.count_summary());
    info!("normalizing RePoE-fork into bundle…");
    normalize_repoe(&snapshot, &mut bundle)?;
    // Alloy affix/required-level back-fill from the curated poe2db table (see normalize::alloy_fixups).
    crate::normalize::apply_alloy_fixups(&mut bundle);

    // ---- Craft of Exile (optional — provides essences/catalysts/weights)
    if opts.skip_coe {
        info!("skipping CoE per BuildOptions");
    } else {
        info!("fetching Craft of Exile (poec_data.json ~2.3MB)…");
        match coe::fetch(&client).await {
            Ok(coe_snap) => {
                info!("{}", coe_snap.count_summary());
                if let Err(e) = normalize_coe(&coe_snap, &mut bundle) {
                    warn!(error = %e, "CoE normalization failed; continuing without CoE data");
                }
            }
            Err(e) => {
                warn!(error = %e, "CoE fetch failed; bundle will lack essences/catalysts/weights");
            }
        }
    }

    // ---- poe2db (optional — provides omens/bones) -----------------------
    if opts.skip_poe2db {
        info!("skipping poe2db per BuildOptions");
    } else {
        info!("fetching poe2db (omens/bones)…");
        match poe2db::fetch(&client).await {
            Ok(p2db_snap) => {
                info!("{}", p2db_snap.count_summary());
                if let Err(e) = normalize_poe2db(&p2db_snap, &mut bundle) {
                    warn!(error = %e, "poe2db normalization failed");
                }
            }
            Err(e) => {
                warn!(error = %e, "poe2db fetch failed; bundle will lack omens/bones");
            }
        }
    }

    // ---- Curated fixtures (always on — embedded in the binary) ----------
    // Phase E: registers desecrated mods + Vaal corruption implicits.
    // These are not scraped because poe2db page layouts shift per patch
    // and tests cannot tolerate flaky network. The fixture is hand-
    // maintained from poe2db's published Desecrated_Modifiers table.
    info!("loading curated fixture data (desecrated + Vaal implicits)…");
    match fixtures::load() {
        Ok(snap) => {
            info!("{}", snap.count_summary());
            if let Err(e) = normalize_fixtures(&snap, &mut bundle) {
                warn!(error = %e, "fixture normalization failed");
            }
        }
        Err(e) => {
            warn!(error = %e, "fixture parse failed; bundle missing desecrated/Vaal mods");
        }
    }

    // ---- Genesis Tree (0.5 — embedded snapshot + curated meta) ----------
    // Only meaningful for 0.5+ bundles; the section stays empty otherwise.
    if opts.game_patch >= PatchVersion::PATCH_0_5_0 {
        info!("loading Genesis Tree (embedded BrequelTree snapshot + curated meta)…");
        match genesis::load() {
            Ok(snap) => {
                info!("{}", snap.count_summary());
                if let Err(e) = normalize_genesis(&snap, &mut bundle) {
                    warn!(error = %e, "genesis normalization failed");
                }
            }
            Err(e) => {
                warn!(error = %e, "genesis fixture parse failed; bundle lacks Genesis Tree");
            }
        }
    }

    // Promote any essence-target mod that didn't already carry the
    // ESSENCE_ONLY flag (Phase E.2 — guarantees the registry's
    // essence pool is complete even when RePoE-fork's is_essence_only
    // boolean missed a join).
    flag_essence_target_mods(&mut bundle);

    // P6 / schema v3: assign explicit tier ordinals over the full mod set
    // (needs every group's members loaded first). Tier 1 = strongest tier of
    // each (mod_group, affix). The engine's inclusive higher-tier weighting
    // consumes these via `ModDefinition::tier_strength_key`.
    let tiered = assign_tier_ordinals(&mut bundle);
    info!(tiered, "assigned explicit tier ordinals");

    if !opts.skip_validation {
        info!("validating bundle…");
        bundle.validate()?;
    }

    info!(
        bases = bundle.base_items.len(),
        classes = bundle.item_classes.len(),
        mods = bundle.mods.len(),
        tags = bundle.tags.len(),
        omens = bundle.omens.entries.len(),
        essences = bundle.essences.entries.len(),
        catalysts = bundle.catalysts.entries.len(),
        bones = bundle.bones.entries.len(),
        weights = bundle.weights.len(),
        "bundle build complete"
    );
    Ok(bundle)
}

/// Cheap epoch-seconds provenance timestamp, matching the format the source
/// modules already use (avoids pulling a date crate into the build path).
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("epoch:{secs}")
}
