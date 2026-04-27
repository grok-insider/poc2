//! Plugin manifest TOML schema.
//!
//! Per ADR-0008 v2:
//!
//! ```toml
//! id = "predicate-meta-build-match"
//! name = "Meta-build matcher"
//! version = "0.1.0"
//! poc2_api_version = "1.0.0"
//! authors = ["alice@example.com"]
//! description = "..."
//!
//! capabilities = ["read_engine", "register_predicate"]
//!
//! [wasm]
//! file = "predicate-meta-build-match.wasm"
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::PluginError;

/// Capabilities a plugin can request. The host enforces the
/// declared list at dispatch time — calls into exports the plugin
/// hasn't requested fail with [`PluginError::MissingCapability`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Read the current Item state passed by the host.
    ReadEngine,
    /// Read the current Valuator's price band.
    ReadMarket,
    /// Read the goal + cost-spent + recommendations-so-far.
    ReadAdvisorState,
    /// Export `eval_predicate(name, item, args) -> bool`.
    RegisterPredicate,
    /// Export `list_strategies() -> Vec<TomlString>`.
    EmitStrategies,
    /// Export `list_rules() -> Vec<TomlString>`.
    EmitRules,
    /// Export `emit_recommendations(state) -> Vec<PluginCandidate>`.
    EmitRecommendations,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WasmSection {
    file: String,
}

/// Parsed `poc2-plugin.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Minimum poc2 API version the plugin requires (semver).
    pub poc2_api_version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    /// Declared capabilities. The host enforces these at dispatch.
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    /// Wasm file path, relative to the manifest's directory.
    #[serde(default, rename = "wasm", deserialize_with = "deserialize_wasm")]
    pub wasm_file: String,
}

fn deserialize_wasm<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let section: WasmSection = Deserialize::deserialize(deserializer)?;
    Ok(section.file)
}

/// Load + parse a manifest TOML file.
pub fn load_manifest(path: &Path) -> Result<PluginManifest, PluginError> {
    let contents = std::fs::read_to_string(path).map_err(PluginError::Io)?;
    let manifest: PluginManifest = toml::from_str(&contents)?;
    if manifest.id.is_empty() {
        return Err(PluginError::Manifest("plugin id is empty".into()));
    }
    if manifest.wasm_file.is_empty() {
        return Err(PluginError::Manifest("wasm.file is empty".into()));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(toml_str: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("poc2-plugin.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(toml_str.as_bytes()).unwrap();
        dir
    }

    #[test]
    fn loads_minimal_manifest() {
        let dir = write_manifest(
            r#"
id = "test-plugin"
name = "Test Plugin"
version = "0.1.0"
poc2_api_version = "1.0.0"
capabilities = ["register_predicate"]
[wasm]
file = "test.wasm"
"#,
        );
        let m = load_manifest(&dir.path().join("poc2-plugin.toml")).unwrap();
        assert_eq!(m.id, "test-plugin");
        assert_eq!(m.wasm_file, "test.wasm");
        assert!(m.capabilities.contains(&Capability::RegisterPredicate));
    }

    #[test]
    fn rejects_empty_id() {
        let dir = write_manifest(
            r#"
id = ""
name = "Test"
version = "0.1.0"
poc2_api_version = "1.0.0"
[wasm]
file = "test.wasm"
"#,
        );
        let r = load_manifest(&dir.path().join("poc2-plugin.toml"));
        assert!(r.is_err());
    }
}
