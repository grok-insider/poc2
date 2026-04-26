//! Newtype identifiers.
//!
//! Each entity in the engine has an opaque string-backed identifier. We use
//! `Box<str>` rather than `String` to save 8 bytes per ID (no excess capacity)
//! and `Arc<str>` is reserved for cases where IDs are heavily shared (the
//! interner in M2.7 will introduce that).
//!
//! The string values are stable identifiers from upstream sources:
//! - `ModId` mirrors RePoE-fork's `mods.json` keys (e.g.,
//!   `"LocalIncreasedEnergyShieldAndLife1"`).
//! - `BaseTypeId` mirrors RePoE-fork's `base_items.json` keys (e.g.,
//!   `"Metadata/Items/Armours/Boots/BootsInt5"`).
//! - `ItemClassId` mirrors `item_class` strings (e.g., `"Boots"`,
//!   `"BodyArmour"`).
//! - `TagId` mirrors RePoE-fork's tag strings (e.g., `"int_armour"`,
//!   `"boots"`, `"caster"`).
//! - `ConceptId` is *our* taxonomy (e.g., `"EnergyShield"`, `"Life"`); see
//!   [`crate::mods::Concept`] for the definition.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_newtype {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Box<str>);

        impl $name {
            pub fn new(s: impl Into<Box<str>>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), &*self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.into())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s.into_boxed_str())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

id_newtype!(
    ModId,
    "Identifier for a [`ModDefinition`](crate::mods::ModDefinition)."
);
id_newtype!(
    BaseTypeId,
    "Identifier for a [`BaseType`](crate::base::BaseType)."
);
id_newtype!(
    ItemClassId,
    "Identifier for a top-level item class (Boots, BodyArmour, ...)."
);
id_newtype!(TagId, "Identifier for a gameplay tag.");
id_newtype!(
    ConceptId,
    "Identifier for a semantic concept (EnergyShield, Life, ...)."
);
id_newtype!(
    StatId,
    "Identifier for a raw stat output (`local_energy_shield_+%`)."
);
id_newtype!(
    ModGroupId,
    "Identifier for a mod-group (mod-exclusivity bucket)."
);
id_newtype!(
    CurrencyId,
    "Identifier for a currency (orb, essence, bone, catalyst, ...)."
);
id_newtype!(OmenId, "Identifier for an omen.");
id_newtype!(
    EssenceId,
    "Identifier for an essence type (Body, Mind, Flames, ...)."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trip_via_str() {
        let m = ModId::new("LocalIncreasedEnergyShieldAndLife1");
        assert_eq!(m.as_str(), "LocalIncreasedEnergyShieldAndLife1");
        assert_eq!(m.to_string(), "LocalIncreasedEnergyShieldAndLife1");
    }

    #[test]
    fn id_serde_round_trip() {
        let m = ModId::from("FooBar");
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#""FooBar""#);
        let back: ModId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn ids_are_distinct_types() {
        // Won't compile if these are aliases — type system enforces separation.
        let _m: ModId = "x".into();
        let _b: BaseTypeId = "x".into();
        // Cannot do `let m: ModId = b;` — different types.
    }
}
