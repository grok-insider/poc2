//! High-level pipeline orchestration: fetch all sources, normalize, validate, return a Bundle.

use poc2_data::Bundle;
use poc2_engine::PatchVersion;
use tracing::info;

use crate::error::PipelineResult;
use crate::http::make_client;
use crate::normalize::normalize_repoe;
use crate::sources::repoe;

/// Pipeline build options.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub game_patch: PatchVersion,
    pub built_by: String,
    /// Whether to skip the cross-reference validation step (useful when
    /// upstream is partially populated; defaults to `false`).
    pub skip_validation: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            game_patch: PatchVersion::PATCH_0_4_0,
            built_by: format!("poc2-pipeline@{}", env!("CARGO_PKG_VERSION")),
            skip_validation: false,
        }
    }
}

/// Build a complete bundle by pulling from every source we currently support.
///
/// At M2.3 we only pull RePoE-fork. M2.4-M2.6 add Craft of Exile (weights),
/// poe2db.tw (omens/essences/bones), and GGG `/trade/data/stats`.
pub async fn build_bundle(opts: BuildOptions) -> PipelineResult<Bundle> {
    info!(?opts.game_patch, "starting bundle build");
    let client = make_client();

    let mut bundle = Bundle::empty(opts.game_patch, opts.built_by);

    info!("fetching RePoE-fork…");
    let snapshot = repoe::fetch(&client).await?;
    info!("{}", snapshot.count_summary());

    info!("normalizing RePoE-fork into bundle…");
    normalize_repoe(&snapshot, &mut bundle)?;

    if !opts.skip_validation {
        info!("validating bundle…");
        bundle.validate()?;
    }

    info!(
        bases = bundle.base_items.len(),
        classes = bundle.item_classes.len(),
        mods = bundle.mods.len(),
        tags = bundle.tags.len(),
        "bundle build complete"
    );
    Ok(bundle)
}
