//! Source provenance for a bundle.
//!
//! Records which upstream revisions / URLs / cache snapshots produced this
//! bundle. Downstream tools and humans use this to:
//! - Diagnose data drift between two bundles
//! - Cite specific sources in advisor explanations
//! - Audit license compliance (per [ADR-0003](../../../docs/adr/0003-data-sources.md))

use serde::{Deserialize, Serialize};

/// One upstream source's revision identifier at bundle build time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRevision {
    /// Source name (e.g., `"repoe-fork"`, `"craftofexile"`, `"poe2db"`).
    pub name: String,
    /// Identifier the upstream uses to mark this revision.
    /// - For Git repos: commit SHA (`abc123...`).
    /// - For HTTP endpoints: ETag / Last-Modified / version field.
    /// - For local files: SHA-256 of the file at fetch time.
    pub revision: String,
    /// Optional source URL that was fetched.
    pub url: Option<String>,
    /// ISO 8601 UTC timestamp of fetch.
    pub fetched_at: String,
}

/// Map of source name → revision for the entire bundle build.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceRevisions(pub Vec<SourceRevision>);

impl SourceRevisions {
    pub fn get(&self, name: &str) -> Option<&SourceRevision> {
        self.0.iter().find(|r| r.name == name)
    }
}
