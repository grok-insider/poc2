//! `canapply` command — port of the desktop `check_can_apply` Tauri command.
//!
//! Given an [`Item`] and a currency id, ask the resolved [`Currency`] whether it
//! can apply (its `can_apply_to` precondition check) and return a serde-stable
//! view of the verdict. All Tauri/`tauri::State` plumbing is dropped: the caller
//! passes the engine's [`CurrencyResolver`] by reference.

use poc2_engine::currency::CurrencyResolver;
use poc2_engine::ids::CurrencyId;
use poc2_engine::item::Item;
use serde::Serialize;

/// Mirror of [`poc2_engine::CannotApply`] for serde-stable IPC. Each
/// variant carries the data the UI needs to render a friendly message;
/// the leading `kind` tag matches the discriminator on the TS side.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CannotApplyView {
    /// Action is applicable — no obstacle.
    Ok,
    /// Currency rejected because it doesn't accept the item's rarity.
    WrongRarity {
        item_rarity: String,
        expected: Vec<String>,
    },
    /// All affix slots of the relevant kind are full.
    NoOpenSlots { affix: String },
    /// Item is corrupted and the currency can't apply.
    Corrupted,
    /// Item is mirrored and cannot be modified.
    Mirrored,
    /// Hinekora's Lock is already active.
    AlreadyLocked,
    /// Fracture refused — item has fewer than 4 visible mods.
    FractureRequiresFourMods { current: u32 },
    /// Recombinator inputs don't share base / ilvl.
    RecombinatorInputMismatch,
    /// Free-form fallback for variants the v2 IPC hasn't enumerated yet.
    Other { message: String },
    /// Currency id wasn't in the engine's resolver.
    UnknownCurrency,
}

fn rarity_label(r: poc2_engine::Rarity) -> &'static str {
    match r {
        poc2_engine::Rarity::Normal => "normal",
        poc2_engine::Rarity::Magic => "magic",
        poc2_engine::Rarity::Rare => "rare",
        poc2_engine::Rarity::Unique => "unique",
    }
}

fn cannot_apply_to_view(reason: poc2_engine::CannotApply) -> CannotApplyView {
    use poc2_engine::CannotApply;
    match reason {
        CannotApply::WrongRarity {
            item_rarity,
            expected,
        } => CannotApplyView::WrongRarity {
            item_rarity: rarity_label(item_rarity).to_string(),
            expected: expected
                .iter()
                .map(|r| rarity_label(r).to_string())
                .collect(),
        },
        CannotApply::NoOpenSlots { affix } => CannotApplyView::NoOpenSlots {
            affix: format!("{affix:?}").to_lowercase(),
        },
        CannotApply::Corrupted => CannotApplyView::Corrupted,
        CannotApply::Mirrored => CannotApplyView::Mirrored,
        CannotApply::AlreadyLocked => CannotApplyView::AlreadyLocked,
        CannotApply::FractureRequiresFourMods { current } => {
            CannotApplyView::FractureRequiresFourMods {
                #[allow(clippy::cast_possible_truncation)]
                current: current as u32,
            }
        }
        CannotApply::RecombinatorInputMismatch => CannotApplyView::RecombinatorInputMismatch,
        CannotApply::Other(s) => CannotApplyView::Other {
            message: s.to_string(),
        },
    }
}

/// Check whether `currency` (by id string) can apply to `item`.
///
/// Resolves the currency through the engine's [`CurrencyResolver`]; an
/// unrecognized id yields [`CannotApplyView::UnknownCurrency`]. Otherwise the
/// currency's `can_apply_to` precondition is evaluated and mapped to a view.
pub fn check_can_apply(
    resolver: &dyn CurrencyResolver,
    item: &Item,
    currency: &str,
) -> CannotApplyView {
    let id = CurrencyId::from(currency);
    let Some(currency) = resolver.resolve(&id) else {
        return CannotApplyView::UnknownCurrency;
    };
    match currency.can_apply_to(item) {
        Ok(()) => CannotApplyView::Ok,
        Err(reason) => cannot_apply_to_view(reason),
    }
}
