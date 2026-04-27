//! Re-export of weight types now owned by `poc2-engine`.
//!
//! These types moved into the engine in v3 so [`poc2_engine::ModRegistry`]
//! can build weight indices at construction time. The `poc2_data::weights`
//! module is preserved as a back-compat shim — bundle serialization
//! callers and pipeline normalizers can keep their existing `use
//! poc2_data::weights::{...}` imports unchanged.

pub use poc2_engine::weights::{Confidence, WeightObservation, WeightScope};
