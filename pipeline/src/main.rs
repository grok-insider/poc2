//! poc2-pipeline — CLI entry.
//!
//! Subcommands:
//! - `build` — pull all sources and emit a versioned bundle
//! - `info`  — load an existing bundle and print summary stats

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use poc2_engine::PatchVersion;
use poc2_pipeline::{build_bundle, BuildOptions};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "poc2-pipeline",
    version,
    about = "Path of Crafting 2 — data bundle builder"
)]
struct Cli {
    /// Set log level (overrides `RUST_LOG`).
    #[arg(long, global = true)]
    log: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build a bundle from upstream sources and write it to disk.
    Build {
        /// Output path. `.gz` suffix → gzip-compressed.
        #[arg(short, long, default_value = "poc2.bundle.json.gz")]
        out: PathBuf,
        /// Game patch the bundle is built against.
        #[arg(long, default_value = "0.4.0")]
        patch: String,
        /// Pretty-print JSON (ignored for `.gz`).
        #[arg(long)]
        pretty: bool,
        /// Skip cross-reference validation.
        #[arg(long)]
        skip_validation: bool,
    },
    /// Load an existing bundle and print its summary.
    Info {
        /// Bundle path. `.gz` auto-detected.
        bundle: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log.as_deref());

    match cli.command {
        Command::Build {
            out,
            patch,
            pretty,
            skip_validation,
        } => {
            let game_patch: PatchVersion = patch.parse()?;
            let opts = BuildOptions {
                game_patch,
                built_by: format!("poc2-pipeline@{}", env!("CARGO_PKG_VERSION")),
                skip_validation,
            };
            let bundle = build_bundle(opts).await?;
            poc2_data::io::write_bundle(&bundle, &out, pretty)?;
            tracing::info!(out = %out.display(), "bundle written");
        }
        Command::Info { bundle } => {
            let b = poc2_data::io::read_bundle(&bundle)?;
            println!("schema_version    : {}", b.header.schema_version);
            println!("engine_schema     : {}", b.header.engine_schema);
            println!("game_patch        : {}", b.header.game_patch);
            println!("built_at          : {}", b.header.built_at);
            println!("built_by          : {}", b.header.built_by);
            println!("item_classes      : {}", b.item_classes.len());
            println!("base_items        : {}", b.base_items.len());
            println!("tags              : {}", b.tags.len());
            println!("concepts          : {}", b.concepts.len());
            println!("mods              : {}", b.mods.len());
            println!("weights           : {}", b.weights.len());
            println!("synergy_edges     : {}", b.synergy_edges.len());
            println!("synergy_overrides : {}", b.synergy_overrides.len());
            println!("mods_by_base      : {}", b.mods_by_base.len());
            println!("source revisions  : {}", b.header.sources.0.len());
            for s in &b.header.sources.0 {
                println!("  • {} = {}", s.name, s.revision);
            }
        }
    }
    Ok(())
}

fn init_tracing(level: Option<&str>) {
    let filter = if let Some(lvl) = level {
        EnvFilter::new(lvl)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,poc2=debug"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
