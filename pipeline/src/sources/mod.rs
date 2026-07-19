//! Data sources — one module per upstream.
//!
//! Each source is responsible for:
//! - Fetching its raw representation
//! - Returning a typed in-memory snapshot
//! - Reporting its provenance ([`poc2_data::SourceRevision`])
//!
//! Normalization (raw → bundle) lives in `crate::normalize`.

pub mod coe;
pub mod fixtures;
pub mod genesis;
pub mod poe2db;
pub mod repoe;
pub mod trade;
