/**
 * Build an ItemPopupModel from the craft-state Item + optional clipboard
 * parse metadata (for rare/unique names and class).
 */

import { humanizeId, humanizeModId, modValue } from "@/lib/format";
import type { Item, ModRoll } from "@/lib/types";
import { emptyItemPopupModel, type ItemPopupKind, type ItemPopupModel } from "./model";
import { splitModValues } from "./fromClipboard";

export interface ItemPopupFromItemOpts {
  /** Rare/unique custom name when known (from clipboard parse). */
  itemName?: string | null;
  itemClassId?: string | null;
  /** Extra unresolved clipboard lines shown as notes. */
  unresolvedLines?: string[];
}

function kindFromItem(item: Item): ItemPopupKind {
  // Craft state only carries armour/weapon rarities; currency/gems arrive via clipboard.
  return item.rarity;
}

function modLine(m: ModRoll): string {
  const label = humanizeModId(m.mod_id);
  const v = modValue(m);
  let text = v ? `${label} ${v}` : label;
  if (m.is_fractured) text += " (fractured)";
  if (m.kind === "crafted") text += " (crafted)";
  return text;
}

export function itemPopupFromItem(item: Item, opts: ItemPopupFromItemOpts = {}): ItemPopupModel {
  const kind = kindFromItem(item);
  const baseName = item.base_display_name ?? humanizeId(item.base);
  const doubleLine = kind === "rare" || kind === "unique";
  const name =
    doubleLine && opts.itemName
      ? opts.itemName
      : kind === "magic" && opts.itemName
        ? opts.itemName
        : baseName;
  const typeLine = doubleLine ? baseName : undefined;

  const properties = [];
  if (opts.itemClassId) {
    properties.push({ text: humanizeId(opts.itemClassId) });
  }
  properties.push({ label: "Item Level", value: String(item.ilvl) });
  if (item.quality > 0) {
    properties.push({ label: "Quality", value: `+${item.quality}%` });
  }

  const sections: ItemPopupModel["sections"] = [];
  if (item.implicits.length > 0) {
    sections.push({
      type: "mods",
      lines: item.implicits.map((m) => splitModValues(modLine(m))),
    });
  }
  const explicits = [...item.prefixes, ...item.suffixes, ...item.enchantments];
  if (explicits.length > 0) {
    sections.push({
      type: "mods",
      lines: explicits.map((m) => splitModValues(modLine(m))),
    });
  } else if (item.implicits.length === 0) {
    sections.push({
      type: "help",
      text: "No explicit modifiers.",
    });
  }

  if (opts.unresolvedLines?.length) {
    sections.push({
      type: "help",
      text: `${opts.unresolvedLines.length} line(s) not recognised as modifiers.`,
    });
  }

  return emptyItemPopupModel({
    kind,
    doubleLine,
    name,
    typeLine,
    properties,
    requirements: [],
    sections,
    flags: {
      corrupted: item.corrupted,
      mirrored: item.mirrored,
      sanctified: item.sanctified,
      unidentified: false,
    },
  });
}
