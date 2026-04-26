//! Lower a poe2db [`Poe2dbSnapshot`] into a [`Bundle`] — populates the
//! `omens` and `bones` BundleSection entries.

use poc2_data::Bundle;
use serde_json::json;
use tracing::info;

use crate::error::PipelineResult;
use crate::sources::poe2db::Poe2dbSnapshot;

#[allow(clippy::unnecessary_wraps)] // forward-compat with future fallible joins
pub fn normalize_poe2db(snapshot: &Poe2dbSnapshot, bundle: &mut Bundle) -> PipelineResult<()> {
    info!("normalizing poe2db snapshot…");

    let omens: Vec<serde_json::Value> = snapshot
        .omens
        .iter()
        .map(|o| {
            json!({
                "id": o.id,
                "name": o.name,
                "description": o.description,
                "icon_url": o.icon_url,
            })
        })
        .collect();
    bundle.omens.section_version = 1;
    bundle.omens.entries = omens;
    info!(count = bundle.omens.entries.len(), "omens populated");

    let bones: Vec<serde_json::Value> = snapshot
        .bones
        .iter()
        .map(|b| {
            json!({
                "id": b.id,
                "name": b.name,
                "size": b.size,
                "subtype": b.subtype,
            })
        })
        .collect();
    bundle.bones.section_version = 1;
    bundle.bones.entries = bones;
    info!(count = bundle.bones.entries.len(), "bones populated");

    bundle.header.sources.0.extend(snapshot.revisions.0.clone());
    Ok(())
}
