//! Pipeline error types.

use thiserror::Error;

pub type PipelineResult<T> = Result<T, PipelineError>;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("http error fetching {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("HTTP {status} from {url}: {body}")]
    HttpStatus {
        url: String,
        status: u16,
        body: String,
    },

    #[error("json parse error from {url}: {source}")]
    JsonParse {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("source `{name}` produced unexpected shape: {detail}")]
    SourceShape { name: &'static str, detail: String },

    #[error("normalization failed: {0}")]
    Normalize(String),

    #[error("data error: {0}")]
    Data(#[from] poc2_data::DataError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("{message}")]
    Other { message: String },
}
