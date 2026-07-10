/** Core item presentation types — shared by GUI + overlay. */

import type { ItemPopupKind, ItemPopupModel } from "@/lib/itemPopup/model";

/** A single affix/mod line with optional advanced-format metadata. */
export interface ItemViewMod {
  text: string;
  /** White-highlighted roll fragments. */
  values?: string[];
  /** Left rail label in advanced tooltips: PREFIX / SUFFIX / IMPLICIT / … */
  side?: "implicit" | "prefix" | "suffix" | "unique" | "corruption" | "rune" | "enchant" | "crafted" | "desecrated";
  /** Right rail tier / unique marker: T8, I, U, E, … */
  tierLabel?: string;
  /** Raw advanced header if present. */
  advancedHeader?: string;
}

export interface ItemViewCapture {
  kind: ItemPopupKind;
  itemClass: string | null;
  rarity: string;
  name: string;
  typeLine?: string;
  properties: Array<{ label?: string; value?: string; text?: string }>;
  requirements: string[];
  sockets?: string;
  itemLevel?: number | null;
  /** Mod lines in display order. */
  mods: ItemViewMod[];
  grantsSkill?: string;
  flavour?: string;
  help?: string;
  flags: {
    corrupted: boolean;
    mirrored: boolean;
    sanctified: boolean;
    unidentified: boolean;
  };
  /** True when advanced `{ Prefix Modifier … }` headers were seen. */
  advanced: boolean;
  /** Original clipboard (for debugging). */
  rawText: string;
}

export interface UniqueCatalogEntry {
  name: string;
  baseType: string;
  artRel?: string | null;
  artSourceUrl?: string | null;
  requirements: string[];
  grantsSkill?: string | null;
  implicits: string[];
  explicits: string[];
  flavour?: string | null;
}

export interface UniqueCatalog {
  version: number;
  source?: string;
  fetched_at?: string;
  entries: Record<string, UniqueCatalogEntry>;
}

export interface ItemView {
  model: ItemPopupModel;
  artUrl: string | null;
  /** Whether a unique catalog entry was applied. */
  uniqueMatched: boolean;
  uniqueKey?: string;
  capture: ItemViewCapture;
}

export type ItemViewSource =
  | { type: "clipboard"; text: string }
  | { type: "unique"; name: string; captureText?: string };
