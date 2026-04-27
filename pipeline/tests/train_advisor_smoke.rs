//! M16.6 — train-advisor binary smoke test.
//!
//! Drives the binary through one tiny goal at low samples to verify the
//! end-to-end pipeline (corpus parse → training → JSON output)
//! compiles, links, and produces a syntactically-valid artefact.
//!
//! Production-scale training (100k samples × 50 goals) is operator-driven
//! — this test only validates the plumbing.
//!
//! Reference: `docs/81-engine-training-and-rule-encoding-plan.md` §6.6
//! Tier 3.6.

use std::path::PathBuf;
use std::process::Command;

fn target_binary() -> PathBuf {
    // CARGO_MANIFEST_DIR is `pipeline/`. The binary lives at
    // `target/debug/train-advisor` relative to the workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
        .join("train-advisor")
}

fn ensure_binary_built() {
    if !target_binary().exists() {
        // First test invocation builds the binary in-place.
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "poc2-pipeline", "--bin", "train-advisor"])
            .status()
            .expect("cargo build train-advisor");
        assert!(status.success(), "binary build failed");
    }
}

#[test]
fn train_advisor_runs_on_minimal_corpus_and_produces_json() {
    ensure_binary_built();
    let tmp_corpus = std::env::temp_dir().join("poc2_train_advisor_smoke_corpus.toml");
    let tmp_out = std::env::temp_dir().join("poc2_train_advisor_smoke_out.json");

    // Tiny corpus: one goal, minimal target.
    std::fs::write(
        &tmp_corpus,
        r#"
[[goal]]
id = "smoke-goal-helmet"
display_name = "Smoke Goal Helmet"
item_class = "Helmet"
ilvl = 82
budget_div = 50.0

[[goal.target.prefixes]]
concept = "Life"
count = 1
allow_hybrid = true
"#,
    )
    .unwrap();

    let output = Command::new(target_binary())
        .arg("--corpus")
        .arg(&tmp_corpus)
        .arg("--out")
        .arg(&tmp_out)
        .arg("--samples")
        .arg("50")
        .arg("--max-states")
        .arg("100")
        .output()
        .expect("run train-advisor");
    assert!(
        output.status.success(),
        "train-advisor exited non-zero: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tmp_out.exists(), "output file should be written");

    let content = std::fs::read_to_string(&tmp_out).unwrap();
    assert!(
        content.starts_with('['),
        "JSON output should start with array bracket; got: {}",
        &content[..content.len().min(80)]
    );
    assert!(
        content.contains("smoke-goal-helmet"),
        "output should mention the goal id"
    );
    assert!(
        content.contains("model_path_length"),
        "output should include path-length model"
    );
    assert!(
        content.contains("model_cost"),
        "output should include cost model"
    );
    assert!(
        content.contains("metrics"),
        "output should include training metrics"
    );

    std::fs::remove_file(&tmp_corpus).ok();
    std::fs::remove_file(&tmp_out).ok();
}
