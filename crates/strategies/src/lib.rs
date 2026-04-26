//! # poc2-strategies
//!
//! Codified strategy library — multi-step crafting recipes loaded from TOML.
//!
//! Each strategy declares preconditions, ordered steps, branch outcomes, and
//! recovery sub-trees up to 3 levels deep (per planning docs). Strategies are
//! data, not code: they ship in the data bundle and can be authored, swapped,
//! and patched without rebuilding the binary.
//!
//! In v1.1+, the [plugin system](../plugin/index.html) (Wasm Component Model)
//! will allow third-party strategy authors to ship strategies as plugins.
//!
//! Seed catalog: 23 strategies from `/docs/33-strategy-library.md` plus the
//! canonical user-authored "Triple T1 Energy Shield Body Armour" test fixture.
//!
//! Stub for M1; real implementation in M3.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod dsl;
pub mod loader;
pub mod registry;
