//! Bundle read/write helpers (JSON, JSON.gz).
//!
//! - `*.bundle.json` — pretty-printed for inspection / diffs in CI
//! - `*.bundle.json.gz` — gzipped for distribution (typical 5-10× shrink)

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use crate::bundle::Bundle;
use crate::error::{DataError, DataResult};

/// Read a bundle from a path. Auto-detects `.gz` suffix.
pub fn read_bundle<P: AsRef<Path>>(path: P) -> DataResult<Bundle> {
    let path = path.as_ref();
    let path_str = path.display().to_string();
    let f = File::open(path).map_err(|e| DataError::Io {
        path: path_str.clone(),
        source: e,
    })?;
    let reader = BufReader::new(f);

    if has_gz_extension(path) {
        #[cfg(feature = "gzip")]
        {
            let gz = flate2::read::GzDecoder::new(reader);
            return serde_json::from_reader(gz).map_err(|e| DataError::Json {
                path: path_str,
                source: e,
            });
        }
        #[cfg(not(feature = "gzip"))]
        {
            return Err(DataError::Gzip("gzip feature disabled".into()));
        }
    }

    serde_json::from_reader(reader).map_err(|e| DataError::Json {
        path: path_str,
        source: e,
    })
}

fn has_gz_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gz"))
}

/// Write a bundle to a path. `pretty` controls indentation; ignored for `.gz`.
pub fn write_bundle<P: AsRef<Path>>(bundle: &Bundle, path: P, pretty: bool) -> DataResult<()> {
    let path = path.as_ref();
    let path_str = path.display().to_string();
    let f = File::create(path).map_err(|e| DataError::Io {
        path: path_str.clone(),
        source: e,
    })?;

    if has_gz_extension(path) {
        #[cfg(feature = "gzip")]
        {
            let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            serde_json::to_writer(&mut gz, bundle).map_err(|e| DataError::Json {
                path: path_str.clone(),
                source: e,
            })?;
            gz.finish().map_err(|e| DataError::Io {
                path: path_str,
                source: e,
            })?;
            return Ok(());
        }
        #[cfg(not(feature = "gzip"))]
        {
            return Err(DataError::Gzip("gzip feature disabled".into()));
        }
    }

    let mut writer = BufWriter::new(f);
    if pretty {
        serde_json::to_writer_pretty(&mut writer, bundle).map_err(|e| DataError::Json {
            path: path_str.clone(),
            source: e,
        })?;
    } else {
        serde_json::to_writer(&mut writer, bundle).map_err(|e| DataError::Json {
            path: path_str.clone(),
            source: e,
        })?;
    }
    writer.flush().map_err(|e| DataError::Io {
        path: path_str,
        source: e,
    })?;
    Ok(())
}
