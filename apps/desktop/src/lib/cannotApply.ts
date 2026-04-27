// Helper for rendering the engine's structured `CannotApply` reason.
//
// The OutcomeDialog and AdvisorPanel both surface "cannot apply"
// messages. They call the `check_can_apply` Tauri command (mocked in
// browser preview) and feed the result into `formatCannotApply` to get
// a single user-facing line.
//
// Phase A.2 of `docs/80-crafter-helper-v2-plan.md` made this an IPC
// surface so the badge text matches the engine's verdict instead of
// drifting via client-side heuristics.

import type { CannotApplyView, Item } from './types';
import { invoke } from './tauri';

export async function checkCanApply(
  item: Item,
  currency: string,
): Promise<CannotApplyView> {
  return invoke<CannotApplyView>('check_can_apply', {
    args: { item, currency },
  });
}

/**
 * Render the engine's `CannotApply` reason as a one-liner.
 * Returns `null` when the action is applicable.
 */
export function formatCannotApply(reason: CannotApplyView): string | null {
  switch (reason.kind) {
    case 'ok':
      return null;
    case 'wrong_rarity': {
      const expected = reason.expected.length
        ? reason.expected.join(' / ')
        : 'no compatible rarity';
      return `wrong rarity — item is ${reason.item_rarity}, currency expects ${expected}`;
    }
    case 'no_open_slots':
      return `no open ${reason.affix} slot`;
    case 'corrupted':
      return 'item is corrupted';
    case 'mirrored':
      return 'item is mirrored';
    case 'already_locked':
      return "Hinekora's Lock already active";
    case 'fracture_requires_four_mods':
      return `Fracture needs ≥ 4 visible mods (currently ${reason.current})`;
    case 'recombinator_input_mismatch':
      return 'recombinator inputs must share base and ilvl';
    case 'other':
      return reason.message;
    case 'unknown_currency':
      return 'unknown currency id (bundle missing this entry?)';
  }
}
