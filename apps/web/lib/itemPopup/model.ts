/** View-model for poe2db-style item tooltips (`.poe-pop`). */

export type ItemPopupKind =
  | "normal"
  | "magic"
  | "rare"
  | "unique"
  | "currency"
  | "gem";

export interface ItemPopupProperty {
  /** Full line text when no separate value (e.g. "Ezomyte Gloves"). */
  text?: string;
  /** Label before colon when split (e.g. "Armour"). */
  label?: string;
  /** Value after colon (e.g. "15"). */
  value?: string;
}

export interface ItemPopupModLine {
  /** Full display text for the mod line. */
  text: string;
  /**
   * Numeric roll fragments to render white inside the blue mod line.
   * When empty, the whole line is blue.
   */
  values?: string[];
  /** Advanced tooltip left rail (PREFIX / SUFFIX / IMPLICIT / …). */
  side?:
    | "implicit"
    | "prefix"
    | "suffix"
    | "unique"
    | "corruption"
    | "rune"
    | "enchant"
    | "crafted"
    | "desecrated";
  /** Advanced tooltip right rail (T8, I, U, E, …). */
  tierLabel?: string;
}

export type ItemPopupSection =
  | { type: "mods"; lines: ItemPopupModLine[] }
  | { type: "secDescr"; text: string }
  | { type: "flavour"; text: string }
  | { type: "help"; text: string };

export interface ItemPopupModel {
  kind: ItemPopupKind;
  /** True → 54px double-line header (rare/unique/gem). */
  doubleLine: boolean;
  /** Top title (or sole title for single-line headers). */
  name: string;
  /** Second title line (base type, "Support", etc.). */
  typeLine?: string;
  properties: ItemPopupProperty[];
  requirements: string[];
  sections: ItemPopupSection[];
  flags: {
    corrupted: boolean;
    mirrored: boolean;
    sanctified: boolean;
    unidentified: boolean;
  };
}

export function emptyItemPopupModel(partial?: Partial<ItemPopupModel>): ItemPopupModel {
  return {
    kind: "normal",
    doubleLine: false,
    name: "Item",
    properties: [],
    requirements: [],
    sections: [],
    flags: {
      corrupted: false,
      mirrored: false,
      sanctified: false,
      unidentified: false,
    },
    ...partial,
  };
}

/** CSS class suffix for `.poe-pop--{kind}`. */
export function poePopKindClass(kind: ItemPopupKind): string {
  return `poe-pop--${kind}`;
}
