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
        #[arg(long, default_value = "0.5.0")]
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
    /// Check whether upstream game data moved since the last recorded state
    /// (the trigger for the automated data-refresh loop — ADR-0012). Reads the
    /// PoE2 patch pointer + RePoE-fork content hashes, compares to the state
    /// file, and (optionally) updates it. Exit code `0` = no change, `10` =
    /// change detected, so CI can branch on `$?`.
    Watch {
        /// Path to the committed upstream-state file.
        #[arg(long, default_value = "pipeline/data/upstream_state.json")]
        state: PathBuf,
        /// Write the freshly-observed state back to `--state` when a change is
        /// detected (the workflow commits this alongside the rebuilt bundle).
        #[arg(long)]
        write: bool,
        /// Emit the machine-readable report as JSON to this path (for the CI
        /// step that opens the PR). Defaults to none (human log only).
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Diff two bundles semantically (added/removed/changed mods, bases, tags,
    /// and section entries) and render a markdown changelog — the body of the
    /// auto-refresh PR.
    DiffBundle {
        /// The baseline (old) bundle. `.gz` auto-detected.
        old: PathBuf,
        /// The candidate (new) bundle. `.gz` auto-detected.
        new: PathBuf,
        /// Write the markdown changelog here. Defaults to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the raw diff as JSON here (CI artifact).
        #[arg(long)]
        json: Option<PathBuf>,
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
    /// Score every unmatched CoE mod against engine mods and emit a
    /// TOML fragment of suggested `[[alias]]` blocks. Operators paste
    /// the reviewed output into `pipeline/data/coe_aliases.toml`.
    CoeAliasesSuggest {
        /// Bundle path to load engine mods from. `.gz` auto-detected.
        bundle: PathBuf,
        /// Optional local CoE JSON file (skips the network fetch).
        #[arg(long)]
        coe_file: Option<PathBuf>,
        /// How many candidates to print per unmatched name.
        #[arg(long, default_value = "3")]
        top_k: usize,
        /// Write the rendered TOML to this file. Defaults to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
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
        Command::Watch {
            state,
            write,
            report,
        } => {
            let code = run_watch(&state, write, report.as_deref()).await?;
            // Non-zero-but-defined exit so CI can branch: 10 = change detected.
            std::process::exit(code);
        }
        Command::DiffBundle {
            old,
            new,
            out,
            json,
        } => {
            run_diff_bundle(&old, &new, out.as_deref(), json.as_deref())?;
        }
        Command::DiagnoseCoe {
            bundle,
            limit,
            coe_file,
        } => {
            run_diagnose_coe(&bundle, limit, coe_file.as_deref()).await?;
        }
        Command::CoeAliasesSuggest {
            bundle,
            coe_file,
            top_k,
            out,
        } => {
            run_coe_aliases_suggest(&bundle, coe_file.as_deref(), top_k, out.as_deref()).await?;
        }
    }
    Ok(())
}

/// Load a CoE snapshot from `--coe-file` (offline) or by fetching live
/// from craftofexile.com. Shared by `diagnose-coe` and `coe-aliases-suggest`.
async fn load_coe_snapshot_for_subcommand(
    coe_file: Option<&std::path::Path>,
) -> Result<poc2_pipeline::sources::coe::CoeSnapshot> {
    use poc2_pipeline::sources::coe;
    if let Some(path) = coe_file {
        let raw = std::fs::read_to_string(path)?;
        let json_part = raw.trim().strip_prefix("poecd=").unwrap_or(raw.trim());
        let data: coe::CoeData = serde_json::from_str(json_part)?;
        Ok(coe::CoeSnapshot {
            data,
            revisions: poc2_data::SourceRevisions::default(),
        })
    } else {
        let client = poc2_pipeline::http::make_client();
        Ok(coe::fetch(&client).await?)
    }
}

/// Run the upstream change check. Returns the process exit code:
/// `0` = no change, `10` = change detected.
async fn run_watch(
    state_path: &std::path::Path,
    write: bool,
    report_path: Option<&std::path::Path>,
) -> Result<i32> {
    let client = poc2_pipeline::http::make_client();
    let previous = poc2_pipeline::UpstreamState::load(state_path)?;
    let report = poc2_pipeline::watch_check(&client, &previous).await?;

    println!("{}", report.summary());

    if let Some(path) = report_path {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(path, json)?;
        tracing::info!(report = %path.display(), "wrote watch report JSON");
    }

    if report.changed && write {
        report.current_state.save(state_path)?;
        tracing::info!(state = %state_path.display(), "updated upstream state");
    } else if report.changed {
        tracing::info!("change detected; re-run with --write to persist the new state");
    }

    Ok(if report.changed { 10 } else { 0 })
}

/// Diff two bundles and render the markdown changelog.
fn run_diff_bundle(
    old_path: &std::path::Path,
    new_path: &std::path::Path,
    out: Option<&std::path::Path>,
    json: Option<&std::path::Path>,
) -> Result<()> {
    let old = poc2_data::io::read_bundle(old_path)?;
    let new = poc2_data::io::read_bundle(new_path)?;
    let diff = poc2_pipeline::diff_bundles(&old, &new);

    tracing::info!(
        total = diff.total_changes(),
        mods = diff.mods.total(),
        bases = diff.bases.total(),
        tags = diff.tags.total(),
        "bundle diff computed"
    );

    if let Some(path) = json {
        std::fs::write(path, serde_json::to_string_pretty(&diff)?)?;
        tracing::info!(json = %path.display(), "wrote raw diff JSON");
    }

    let md = poc2_pipeline::render_markdown(&diff);
    if let Some(path) = out {
        std::fs::write(path, &md)?;
        println!("wrote markdown changelog to {}", path.display());
    } else {
        print!("{md}");
    }
    Ok(())
}

async fn run_diagnose_coe(
    bundle_path: &std::path::Path,
    limit: usize,
    coe_file: Option<&std::path::Path>,
) -> Result<()> {
    use poc2_pipeline::normalize::coe_to_bundle::unmatched_coe_mods;
    let bundle = poc2_data::io::read_bundle(bundle_path)?;
    let snapshot = load_coe_snapshot_for_subcommand(coe_file).await?;

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

async fn run_coe_aliases_suggest(
    bundle_path: &std::path::Path,
    coe_file: Option<&std::path::Path>,
    top_k: usize,
    out: Option<&std::path::Path>,
) -> Result<()> {
    use poc2_pipeline::normalize::coe_to_bundle::{
        render_alias_suggestions_toml, suggest_aliases_for_unmatched,
    };
    let bundle = poc2_data::io::read_bundle(bundle_path)?;
    let snapshot = load_coe_snapshot_for_subcommand(coe_file).await?;

    let suggestions = suggest_aliases_for_unmatched(&snapshot, &bundle, top_k);
    let rendered = render_alias_suggestions_toml(&suggestions);

    tracing::info!(
        suggestions = suggestions.len(),
        with_candidates = suggestions
            .iter()
            .filter(|s| !s.candidates.is_empty())
            .count(),
        no_candidates = suggestions
            .iter()
            .filter(|s| s.candidates.is_empty())
            .count(),
        "alias suggester finished"
    );

    if let Some(path) = out {
        std::fs::write(path, &rendered)?;
        println!(
            "wrote {} suggestions to {}",
            suggestions.len(),
            path.display()
        );
    } else {
        print!("{rendered}");
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
