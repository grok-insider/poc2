//! Strategy loader — reads TOML / JSON into [`Strategy`] values.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use thiserror::Error;

use crate::dsl::Strategy;

pub type StrategyResult<T> = Result<T, StrategyError>;

#[derive(Debug, Error)]
pub enum StrategyError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("toml parse error in {path}: {source}")]
    Toml {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("json parse error in {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("strategy validation: {0}")]
    Validation(String),
}

/// Parse a strategy from a TOML string.
pub fn load_strategy_str(toml_str: &str) -> StrategyResult<Strategy> {
    let strategy: Strategy = toml::from_str(toml_str).map_err(|e| StrategyError::Toml {
        path: "<inline>".into(),
        source: e,
    })?;
    validate(&strategy)?;
    Ok(strategy)
}

/// Parse a strategy from a TOML file. Auto-detects `.json` extension to use
/// the JSON parser instead.
pub fn load_strategy_toml<P: AsRef<Path>>(path: P) -> StrategyResult<Strategy> {
    let path = path.as_ref();
    let path_str = path.display().to_string();
    let mut file = File::open(path).map_err(|e| StrategyError::Io {
        path: path_str.clone(),
        source: e,
    })?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| StrategyError::Io {
            path: path_str.clone(),
            source: e,
        })?;

    let is_json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));

    let strategy: Strategy = if is_json {
        serde_json::from_str(&content).map_err(|e| StrategyError::Json {
            path: path_str.clone(),
            source: e,
        })?
    } else {
        toml::from_str(&content).map_err(|e| StrategyError::Toml {
            path: path_str.clone(),
            source: e,
        })?
    };
    validate(&strategy)?;
    Ok(strategy)
}

/// Structural validation — does NOT check semantic correctness vs the
/// engine (that's the executor's job).
///
/// Catches:
/// - Empty step list
/// - Duplicate step ids within a strategy
/// - Dangling step references (on_success / on_failure / branch goto)
/// - Inverted patch ranges
fn validate(s: &Strategy) -> StrategyResult<()> {
    if s.steps.is_empty() {
        return Err(StrategyError::Validation(format!(
            "strategy `{}` has no steps",
            s.id.0
        )));
    }

    if let (Some(min), Some(max)) = (s.patch_min, s.patch_max) {
        if min > max {
            return Err(StrategyError::Validation(format!(
                "strategy `{}` patch range invalid: {min} > {max}",
                s.id.0
            )));
        }
    }

    let ids: ahash::AHashSet<&str> = s.steps.iter().map(|st| st.id.0.as_str()).collect();
    if ids.len() != s.steps.len() {
        return Err(StrategyError::Validation(format!(
            "strategy `{}` has duplicate step ids",
            s.id.0
        )));
    }

    for st in &s.steps {
        if let Some(target) = &st.on_success {
            if !ids.contains(target.0.as_str()) {
                return Err(StrategyError::Validation(format!(
                    "step `{}` on_success points to unknown step `{}`",
                    st.id.0, target.0
                )));
            }
        }
        if let Some(target) = &st.on_failure {
            if !ids.contains(target.0.as_str()) {
                return Err(StrategyError::Validation(format!(
                    "step `{}` on_failure points to unknown step `{}`",
                    st.id.0, target.0
                )));
            }
        }
        for hint in &st.recovery {
            if let Some(target) = &hint.goto {
                if !ids.contains(target.0.as_str()) {
                    return Err(StrategyError::Validation(format!(
                        "step `{}` recovery hint points to unknown step `{}`",
                        st.id.0, target.0
                    )));
                }
            }
        }
        // Branch action targets
        if let crate::dsl::Action::Branch(branches) = &st.action {
            for b in branches {
                if !ids.contains(b.goto.0.as_str()) {
                    return Err(StrategyError::Validation(format!(
                        "step `{}` branch points to unknown step `{}`",
                        st.id.0, b.goto.0
                    )));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
id = "test"
name = "test strategy"
patch_min = "0.4.0"
item_classes = ["BodyArmour"]

[[steps]]
id = "S1"
action = { kind = "noop" }
"#;

    #[test]
    fn loads_minimal_strategy() {
        let s = load_strategy_str(MINIMAL).unwrap();
        assert_eq!(s.id.0, "test");
        assert_eq!(s.name, "test strategy");
        assert_eq!(s.steps.len(), 1);
        assert_eq!(s.steps[0].id.0, "S1");
    }

    #[test]
    fn rejects_empty_steps() {
        let toml = r#"
id = "empty"
name = "empty"
steps = []
"#;
        let r = load_strategy_str(toml);
        assert!(r.is_err());
    }

    #[test]
    fn rejects_duplicate_step_ids() {
        let toml = r#"
id = "dup"
name = "dup"

[[steps]]
id = "S1"
action = { kind = "noop" }

[[steps]]
id = "S1"
action = { kind = "noop" }
"#;
        let r = load_strategy_str(toml);
        assert!(r.is_err());
    }

    #[test]
    fn rejects_dangling_step_reference() {
        let toml = r#"
id = "dangle"
name = "dangle"

[[steps]]
id = "S1"
action = { kind = "noop" }
on_success = "S99"
"#;
        let r = load_strategy_str(toml);
        assert!(r.is_err());
    }

    #[test]
    fn loads_strategy_with_branches_and_recovery() {
        let toml = r#"
id = "fancy"
name = "fancy"

[[steps]]
id = "S1"
action = { kind = "apply_currency", currency = "PerfectOrbOfTransmutation" }
on_success = "S2"
on_failure = "S3"

[[steps]]
id = "S2"
action = { kind = "done" }

[[steps]]
id = "S3"
action = { kind = "abandon", reason = "no T1 hit" }
"#;
        let s = load_strategy_str(toml).unwrap();
        assert_eq!(s.steps.len(), 3);
    }
}
