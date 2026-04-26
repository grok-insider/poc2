//! Rule file loader — reads TOML rule files into [`Rule`] values.

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

pub fn load_rule_str(s: &str) -> Result<Rule, RuleError> {
    Ok(toml::from_str(s)?)
}

pub fn load_rule_json(s: &str) -> Result<Rule, RuleError> {
    Ok(serde_json::from_str(s)?)
}
