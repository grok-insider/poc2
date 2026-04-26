//! # poc2-parser
//!
//! Parser for Path of Exile 2 in-game item text (the Ctrl+C clipboard
//! format).
//!
//! The parser is split in two phases:
//!
//! 1. **`text`**: pure string parsing → [`ParsedItem`] (no engine
//!    dependency on mod data; just the literal text fields).
//! 2. **`lower`**: convert a [`ParsedItem`] into a
//!    [`poc2_engine::Item`] using a [`poc2_engine::ModRegistry`] for mod
//!    text → mod id resolution.
//!
//! Phase 1 is deterministic and offline; phase 2 needs the data bundle.
//! Callers without a registry can still use phase 1 to display the
//! parsed item to the user.
//!
//! ## Format reference
//!
//! PoE2's clipboard format is a series of dashed sections:
//!
//! ```text
//! Item Class: Body Armours
//! Rarity: Rare
//! Doom Greaves
//! Expert Wyrmscale Coat
//! --------
//! Quality: +20% (augmented)
//! Armour: 521 (augmented)
//! --------
//! Requirements:
//! Level: 65
//! Str: 96
//! Int: 96
//! --------
//! Item Level: 82
//! --------
//! +25 to Maximum Energy Shield
//! 54% increased Energy Shield
//! --------
//! Corrupted
//! ```
//!
//! Magic and Normal items omit the second name line. Sanctified /
//! Mirrored / Corrupted appear as standalone trailing lines.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::doc_markdown)]

pub mod lower;
pub mod text;

pub use lower::lower_to_item;
pub use text::{parse_clipboard_text, ParseError, ParsedItem, Requirements};
