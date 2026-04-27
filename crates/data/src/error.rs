//! Error types for bundle loading and validation.

use thiserror::Error;

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, Error)]
pub enum DataError {
    #[error(
        "bundle schema version mismatch: bundle declares v{bundle}, loader expects v{expected}. \
         Rebuild the bundle via `cargo run -p poc2-pipeline -- build` to upgrade."
    )]
    SchemaVersionMismatch { bundle: u32, expected: u32 },

    #[error(
        "engine schema version mismatch: bundle declares {bundle}, engine is {expected}; refusing"
    )]
    EngineSchemaMismatch { bundle: u32, expected: u32 },

    #[error("bundle declares game patch {bundle} but loader is configured for {expected}")]
    GamePatchMismatch { bundle: String, expected: String },

    #[error("bundle missing required section: {0}")]
    MissingSection(&'static str),

    #[error("bundle validation failed: {0}")]
    Validation(String),

    #[error("bundle entity {entity_kind} {id} references unknown {ref_kind} {ref_id}")]
    DanglingReference {
        entity_kind: &'static str,
        id: String,
        ref_kind: &'static str,
        ref_id: String,
    },

    #[error("io error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("json parse error in {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("toml parse error in {path}: {source}")]
    Toml {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("gzip error: {0}")]
    Gzip(String),
}
