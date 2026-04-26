//! Rule file loader — reads TOML rule files into [`Rule`] values.
//!
//! Two TOML shapes are supported:
//!
//! 1. **Single-rule TOML** — top-level `id`, `category`, `when`, etc.
//!    Use [`load_rule_str`] / [`load_rule_json`].
//! 2. **Rule-array TOML** — `[[rule]]` arrays, suitable for shipping
//!    a section of the heuristics rulebook as one file.
//!    Use [`load_rules_str`] / [`load_rules_json`].
//!
//! The seed rule catalogue ships as rule-array TOMLs under
//! `crates/rules/seed_rules/<section>.toml`, one per /docs/34 section.

use serde::Deserialize;
use thiserror::Error;

use crate::rule::Rule;

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("json parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Load a single rule from a TOML string.
pub fn load_rule_str(s: &str) -> Result<Rule, RuleError> {
    Ok(toml::from_str(s)?)
}

/// Load a single rule from a JSON string.
pub fn load_rule_json(s: &str) -> Result<Rule, RuleError> {
    Ok(serde_json::from_str(s)?)
}

/// TOML wrapper for a `[[rule]]` array.
#[derive(Debug, Deserialize)]
struct RulesFile {
    #[serde(default, rename = "rule")]
    rules: Vec<Rule>,
}

/// Load a list of rules from a `[[rule]]` array TOML.
pub fn load_rules_str(s: &str) -> Result<Vec<Rule>, RuleError> {
    let f: RulesFile = toml::from_str(s)?;
    Ok(f.rules)
}

/// Load a list of rules from a `[[rule]]` array JSON.
pub fn load_rules_json(s: &str) -> Result<Vec<Rule>, RuleError> {
    let f: RulesFile = serde_json::from_str(s)?;
    Ok(f.rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_empty_rule_array() {
        assert_eq!(load_rules_str("").unwrap().len(), 0);
    }

    #[test]
    fn loads_single_rule_via_array_shape() {
        let toml_str = r#"
            [[rule]]
            id = "test-rule-1"
            category = "fracture"
            when = { rarity = "rare" }
            then = [{ action = { kind = "guidance" }, note = "test note", priority = 100 }]
            explanation = "test"
            source = "test"
        "#;
        let rules = load_rules_str(toml_str).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id.0, "test-rule-1");
    }
}
