//! poc2-pipeline — data bundle builder.
//!
//! Pulls source data from RePoE-fork, Craft of Exile, poe2db.tw, and the GGG
//! trade-stat API; normalizes; cross-validates weights; emits a versioned
//! bundle as JSON / JSON.GZ.
//!
//! Stub for M1. Real implementation in M2.

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("poc2-pipeline {} (stub)", env!("CARGO_PKG_VERSION"));
    tracing::info!("real implementation lands in M2 (Data Pipeline)");
    Ok(())
}
