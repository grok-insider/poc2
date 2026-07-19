//! Patch version handling.
//!
//! Every entity (mod, currency, omen, strategy, rule) carries `patch_min` / `patch_max`
//! so that the same engine binary can correctly evaluate items and bundles
//! across multiple game patches.
//!
//! Format: `MAJOR.MINOR.PATCH[.SUBPATCH]` mirrored from GGG's versioning
//! (e.g., `0.4.0`, `0.4.0c`, `0.5.0`).

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// A PoE2 game patch identifier.
///
/// Internally stored as `(major, minor, patch, subpatch)` where `subpatch` is a
/// lowercase letter mapped to `1..=26` (`a` = 1, `b` = 2, ...) or `0` if absent.
///
/// Serializes as a string (`"0.4.0c"`) for human-readable formats; deserializes
/// from either the string form or the explicit struct form, so TOML and JSON
/// can use whichever is convenient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatchVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub subpatch: u8, // 0 = no subpatch; 1 = a; 2 = b; ...
}

impl Serialize for PatchVersion {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self)
    }
}

impl<'de> Deserialize<'de> for PatchVersion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = PatchVersion;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a patch version string like \"0.4.0\" or \"0.4.0c\"")
            }
            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                s.parse().map_err(serde::de::Error::custom)
            }
            // Accept the struct form too, so existing JSON output round-trips.
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut major = 0u8;
                let mut minor = 0u8;
                let mut patch = 0u8;
                let mut subpatch = 0u8;
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "major" => major = map.next_value()?,
                        "minor" => minor = map.next_value()?,
                        "patch" => patch = map.next_value()?,
                        "subpatch" => subpatch = map.next_value()?,
                        other => {
                            return Err(serde::de::Error::unknown_field(
                                other,
                                &["major", "minor", "patch", "subpatch"],
                            ))
                        }
                    }
                }
                Ok(PatchVersion {
                    major,
                    minor,
                    patch,
                    subpatch,
                })
            }
        }
        d.deserialize_any(V)
    }
}

impl PatchVersion {
    pub const fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
            subpatch: 0,
        }
    }

    pub const fn with_subpatch(major: u8, minor: u8, patch: u8, subpatch_letter: char) -> Self {
        let sp = (subpatch_letter as u8)
            .saturating_sub(b'a')
            .saturating_add(1);
        Self {
            major,
            minor,
            patch,
            subpatch: sp,
        }
    }

    /// 0.4.0 — "The Last of the Druids" / "Fate of the Vaal" league.
    pub const PATCH_0_4_0: Self = Self::new(0, 4, 0);

    /// 0.5.0 — "Return of the Ancients" (scheduled May 29 2026).
    pub const PATCH_0_5_0: Self = Self::new(0, 5, 0);
}

impl PartialOrd for PatchVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PatchVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch, self.subpatch).cmp(&(
            other.major,
            other.minor,
            other.patch,
            other.subpatch,
        ))
    }
}

impl fmt::Display for PatchVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.subpatch == 0 {
            write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
        } else {
            let letter = (b'a' + self.subpatch - 1) as char;
            write!(f, "{}.{}.{}{}", self.major, self.minor, self.patch, letter)
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid patch version: {0}")]
pub struct ParsePatchError(pub String);

impl FromStr for PatchVersion {
    type Err = ParsePatchError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Examples: "0.4.0", "0.4.0c", "0.5.0"
        let mk_err = || ParsePatchError(s.to_string());
        let (numeric_part, subpatch) = match s.chars().last() {
            Some(c) if c.is_ascii_alphabetic() => {
                let sp = (c.to_ascii_lowercase() as u8)
                    .saturating_sub(b'a')
                    .saturating_add(1);
                (&s[..s.len() - 1], sp)
            }
            _ => (s, 0),
        };
        let mut parts = numeric_part.split('.');
        let major: u8 = parts
            .next()
            .ok_or_else(mk_err)?
            .parse()
            .map_err(|_| mk_err())?;
        let minor: u8 = parts
            .next()
            .ok_or_else(mk_err)?
            .parse()
            .map_err(|_| mk_err())?;
        let patch: u8 = parts
            .next()
            .ok_or_else(mk_err)?
            .parse()
            .map_err(|_| mk_err())?;
        if parts.next().is_some() {
            return Err(mk_err());
        }
        Ok(Self {
            major,
            minor,
            patch,
            subpatch,
        })
    }
}

/// Inclusive patch range. `None` for `min` means "any version up to max"; `None` for `max`
/// means "from min onwards (current)".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatchRange {
    pub min: Option<PatchVersion>,
    pub max: Option<PatchVersion>,
}

impl PatchRange {
    pub const ALL: Self = Self {
        min: None,
        max: None,
    };
    pub const fn from(min: PatchVersion) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }
    pub const fn until(max: PatchVersion) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }
    pub const fn between(min: PatchVersion, max: PatchVersion) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }
    /// True if `v` is within this inclusive range.
    pub fn contains(&self, v: PatchVersion) -> bool {
        self.min.is_none_or(|m| v >= m) && self.max.is_none_or(|m| v <= m)
    }
}

/// Which league ruleset is in effect.
///
/// Some items function in **Standard** but not in the current trade /
/// challenge league. Notable 0.5 example: the Recombinator and the Omen of
/// Corruption / Homogenising omens are Standard-only — they still work on
/// migrated characters but cannot be used in the active Runes of Aldur
/// challenge league. The advisor must not recommend Standard-only items when
/// the user is playing the challenge league.
///
/// Carried on [`crate::currency::ApplyContext`] alongside [`PatchVersion`]
/// so the same engine binary evaluates both rulesets. Hardcore variants
/// share crafting rules with their softcore parents, so we only distinguish
/// `Standard` vs `Challenge` for gating purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum League {
    /// Permanent league; legacy items (Recombinator, Corruption omen) still
    /// function.
    Standard,
    /// Current temporary challenge league (Runes of Aldur in 0.5). This is
    /// the default the advisor assumes — it is the trade-relevant ruleset.
    #[default]
    Challenge,
}

impl League {
    /// The league the advisor assumes by default (the current challenge
    /// league). Items disabled in the challenge league are not recommended.
    pub const fn current() -> Self {
        Self::Challenge
    }

    /// True when legacy / Standard-only items (Recombinator, Omen of
    /// Corruption, Homogenising omens in 0.5) are usable.
    pub const fn allows_legacy_items(self) -> bool {
        matches!(self, Self::Standard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic() {
        assert_eq!(
            "0.4.0".parse::<PatchVersion>().unwrap(),
            PatchVersion::new(0, 4, 0)
        );
    }

    #[test]
    fn parse_subpatch() {
        let v: PatchVersion = "0.4.0c".parse().unwrap();
        assert_eq!(v, PatchVersion::with_subpatch(0, 4, 0, 'c'));
        assert_eq!(v.to_string(), "0.4.0c");
    }

    #[test]
    fn ordering() {
        let a: PatchVersion = "0.4.0".parse().unwrap();
        let b: PatchVersion = "0.4.0a".parse().unwrap();
        let c: PatchVersion = "0.4.0c".parse().unwrap();
        let d: PatchVersion = "0.5.0".parse().unwrap();
        assert!(a < b);
        assert!(b < c);
        assert!(c < d);
    }

    #[test]
    fn range_contains() {
        let r = PatchRange::between(PatchVersion::PATCH_0_4_0, PatchVersion::PATCH_0_5_0);
        assert!(r.contains("0.4.5".parse().unwrap()));
        assert!(!r.contains("0.6.0".parse().unwrap()));
    }

    #[test]
    fn league_default_is_challenge() {
        assert_eq!(League::default(), League::Challenge);
        assert_eq!(League::current(), League::Challenge);
    }

    #[test]
    fn league_legacy_gate() {
        assert!(League::Standard.allows_legacy_items());
        assert!(!League::Challenge.allows_legacy_items());
    }

    #[test]
    fn league_serde_round_trips() {
        for l in [League::Standard, League::Challenge] {
            let j = serde_json::to_string(&l).unwrap();
            let back: League = serde_json::from_str(&j).unwrap();
            assert_eq!(back, l);
        }
        // Confirm snake_case wire form.
        assert_eq!(
            serde_json::to_string(&League::Challenge).unwrap(),
            "\"challenge\""
        );
    }
}
