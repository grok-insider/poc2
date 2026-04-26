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
        /// Skip the Craft of Exile fetch (offline / faster mode).
        #[arg(long)]
        skip_coe: bool,
        /// Skip the poe2db scrape.
        #[arg(long)]
        skip_poe2db: bool,
    },
    /// Load an existing bundle and print its summary.
    Info {
        /// Bundle path. `.gz` auto-detected.
        bundle: PathBuf,
    },
    /// Re-fetch the Craft of Exile snapshot and report which mods could
    /// not be joined to engine ModIds (input for new entries in
    /// `pipeline/data/coe_aliases.toml`).
    DiagnoseCoe {
        /// Bundle path to load engine mods from. `.gz` auto-detected.
        bundle: PathBuf,
        /// Limit how many unmatched names to print. `0` = print all.
        #[arg(long, default_value = "50")]
        limit: usize,
        /// Optional local CoE JSON file (skips the network fetch).
        #[arg(long)]
        coe_file: Option<PathBuf>,
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
            skip_coe,
            skip_poe2db,
        } => {
            let game_patch: PatchVersion = patch.parse()?;
            let opts = BuildOptions {
                game_patch,
                built_by: format!("poc2-pipeline@{}", env!("CARGO_PKG_VERSION")),
                skip_validation,
                skip_coe,
                skip_poe2db,
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
            println!("omens             : {}", b.omens.entries.len());
            println!("essences          : {}", b.essences.entries.len());
            println!("catalysts         : {}", b.catalysts.entries.len());
            println!("bones             : {}", b.bones.entries.len());
            println!("synergy_edges     : {}", b.synergy_edges.len());
            println!("synergy_overrides : {}", b.synergy_overrides.len());
            println!("mods_by_base      : {}", b.mods_by_base.len());
            println!("source revisions  : {}", b.header.sources.0.len());
            for s in &b.header.sources.0 {
                println!("  • {} = {}", s.name, s.revision);
            }
        }
        Command::DiagnoseCoe {
            bundle,
            limit,
            coe_file,
        } => {
            run_diagnose_coe(&bundle, limit, coe_file.as_deref()).await?;
        }
    }
    Ok(())
}

async fn run_diagnose_coe(
    bundle_path: &std::path::Path,
    limit: usize,
    coe_file: Option<&std::path::Path>,
) -> Result<()> {
    use poc2_pipeline::normalize::coe_to_bundle::unmatched_coe_mods;
    use poc2_pipeline::sources::coe;
    let bundle = poc2_data::io::read_bundle(bundle_path)?;

    let snapshot = if let Some(path) = coe_file {
        let raw = std::fs::read_to_string(path)?;
        let json_part = raw.trim().strip_prefix("poecd=").unwrap_or(raw.trim());
        let data: coe::CoeData = serde_json::from_str(json_part)?;
        coe::CoeSnapshot {
            data,
            revisions: poc2_data::SourceRevisions::default(),
        }
    } else {
        let client = poc2_pipeline::http::make_client();
        coe::fetch(&client).await?
    };

    let unmatched = unmatched_coe_mods(&snapshot, &bundle);
    let total_coe = snapshot.data.tiers.len();
    let matched = total_coe.saturating_sub(unmatched.len());
    let rate = if total_coe == 0 {
        0.0
    } else {
        matched as f64 / total_coe as f64 * 100.0
    };

    println!(
        "coe→engine join: {matched}/{total_coe} ({rate:.1}%)  unmatched: {}",
        unmatched.len()
    );
    println!(
        "target: ≥ 80%; gap: {} mods",
        80usize.saturating_sub(rate as usize) * total_coe / 100
    );

    let mut by_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in &unmatched {
        *by_freq.entry(name.clone()).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, usize)> = by_freq.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let printed = if limit == 0 {
        sorted.len()
    } else {
        limit.min(sorted.len())
    };
    println!("\ntop {printed} unmatched CoE mod names (add aliases for high-freq entries):");
    for (name, count) in sorted.iter().take(printed) {
        println!("  {count:>4}× {name}");
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
