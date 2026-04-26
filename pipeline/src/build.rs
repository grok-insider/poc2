//! High-level pipeline orchestration: fetch all sources, normalize, validate, return a Bundle.

use poc2_data::Bundle;
use poc2_engine::PatchVersion;
use tracing::{info, warn};

use crate::error::PipelineResult;
use crate::http::make_client;
use crate::normalize::{normalize_coe, normalize_poe2db, normalize_repoe};
use crate::sources::{coe, poe2db, repoe};

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
            game_patch: PatchVersion::PATCH_0_4_0,
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

    // ---- RePoE-fork (mandatory) -----------------------------------------
    info!("fetching RePoE-fork…");
    let snapshot = repoe::fetch(&client).await?;
    info!("{}", snapshot.count_summary());
    info!("normalizing RePoE-fork into bundle…");
    normalize_repoe(&snapshot, &mut bundle)?;

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
