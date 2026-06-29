// Pure helpers for the capture flow — unit-tested directly.

/**
 * First lines PoE2 writes for a copied item, per client language.
 * (Same detection approach as Awakened PoE Trade's HostClipboard.)
 */
const ITEM_FIRST_LINES = [
  "Item Class: ", // en
  "Klasse: ", // de
  "Classe d'objet: ", // fr
  "Clase de objeto: ", // es
  "아이템 종류: ", // ko
  "Классификация: ", // ru
  "Classe do Item: ", // pt-br
  "物品種類: ", // zh-tw
  "物品类别: ", // zh-cn
  "アイテムクラス: ", // ja
];

/** Does this clipboard text look like a PoE2 item copy? */
export function isPoeItemText(text: string): boolean {
  return ITEM_FIRST_LINES.some((l) => text.startsWith(l));
}

export interface CaptureTimings {
  /** Poll interval while waiting for the game to write the clipboard. */
  pollMs: number;
  /** Give up after this long. */
  timeoutMs: number;
  /** Restore the previous clipboard this long after a successful read. */
  restoreAfterMs: number;
}

/** APT-derived defaults (48ms poll / 500ms budget / 120ms restore). */
export const DEFAULT_TIMINGS: CaptureTimings = {
  pollMs: 48,
  timeoutMs: 500,
  restoreAfterMs: 120,
};
