/**
 * Core item presentation pipeline for GUI + overlay.
 *
 * - Unique / named fixed items: match catalog by name; overlay capture deltas
 *   (quality, sockets, runes, corruption, rolled values, ilvl).
 * - Magic / rare / normal: fully from clipboard (including advanced format).
 */

import { emptyItemPopupModel, type ItemPopupModel } from "@/lib/itemPopup/model";
import { splitModValues } from "@/lib/itemPopup/fromClipboard";
import { resolveItemArt, type UniqueIconManifest } from "@/lib/itemArt";
import type { BaseIconManifest } from "@/lib/types";
import { parseCapture } from "./parseCapture";
import { lookupUnique } from "./uniqueCatalog";
import type {
  ItemView,
  ItemViewCapture,
  ItemViewMod,
  ItemViewSource,
  UniqueCatalog,
  UniqueCatalogEntry,
} from "./types";

export interface BuildItemViewOptions {
  baseManifest?: BaseIconManifest | null;
  uniqueManifest?: UniqueIconManifest | null;
  uniqueCatalog?: UniqueCatalog | null;
}

function captureToModel(cap: ItemViewCapture): ItemPopupModel {
  const model = emptyItemPopupModel({
    kind: cap.kind,
    doubleLine: cap.kind === "rare" || cap.kind === "unique" || cap.kind === "gem",
    name: cap.name,
    typeLine: cap.typeLine,
    properties: cap.properties,
    requirements: cap.requirements,
    sections: [],
    flags: cap.flags,
  });

  const modLines = [
    ...(cap.grantsSkill
      ? [
          {
            text: cap.grantsSkill,
            side: "enchant" as const,
          },
        ]
      : []),
    ...cap.mods.map((m) => ({
      text: m.text,
      values: m.values,
      side: m.side,
      tierLabel: m.tierLabel,
    })),
  ];
  if (modLines.length) {
    model.sections.push({ type: "mods", lines: modLines });
  }
  if (cap.flavour) model.sections.push({ type: "flavour", text: cap.flavour });
  if (cap.help) model.sections.push({ type: "help", text: cap.help });
  return model;
}

/**
 * Merge a unique catalog definition with capture-only deltas.
 * Capture supplies: quality/defences (properties), sockets, runes, corruption
 * lines, item level, advanced rolled unique mods when present.
 */
export function mergeUniqueWithCapture(
  entry: UniqueCatalogEntry,
  cap: ItemViewCapture | null,
): ItemPopupModel {
  const properties = [...(cap?.properties ?? [])];
  // Ensure class property exists if capture didn't provide
  if (cap?.itemClass && !properties.some((p) => p.text === cap.itemClass)) {
    // keep capture properties as-is (include ilvl, quality, armour…)
  }

  const requirements =
    cap?.requirements?.length ? cap.requirements : entry.requirements.slice();

  const mods: ItemViewMod[] = [];

  // Capture-only: corruption enhancements, runes (before unique mods)
  if (cap) {
    for (const m of cap.mods) {
      if (m.side === "corruption" || m.side === "rune" || m.text.endsWith("(rune)")) {
        mods.push(m.side ? m : { ...m, side: "rune" });
      }
    }
  }

  if (entry.implicits.length) {
    for (const t of entry.implicits) {
      const s = splitModValues(t);
      mods.push({ text: s.text, values: s.values, side: "implicit", tierLabel: "I" });
    }
  }

  if (cap?.grantsSkill || entry.grantsSkill) {
    mods.push({
      text: cap?.grantsSkill ?? entry.grantsSkill!,
      side: "enchant",
    });
  }

  // Unique explicits: prefer advanced capture unique lines when present;
  // otherwise catalog templates (range form).
  const captureUnique = cap?.mods.filter((m) => m.side === "unique") ?? [];
  if (captureUnique.length > 0) {
    mods.push(...captureUnique);
  } else {
    for (const t of entry.explicits) {
      const s = splitModValues(t);
      mods.push({ text: s.text, values: s.values, side: "unique", tierLabel: "U" });
    }
  }

  // Also include capture mods that aren't unique/rune/corruption if advanced
  // paste had extra lines not classified (fallback)
  if (cap && captureUnique.length === 0 && cap.mods.length > entry.explicits.length) {
    // Prefer catalog; don't double-append unclassified
  }

  const model = emptyItemPopupModel({
    kind: "unique",
    doubleLine: true,
    name: entry.name,
    typeLine: entry.baseType,
    properties,
    requirements,
    sections: [],
    flags: cap?.flags ?? {
      corrupted: false,
      mirrored: false,
      sanctified: false,
      unidentified: false,
    },
  });
  if (mods.length) {
    model.sections.push({
      type: "mods",
      lines: mods.map((m) => ({
        text: m.text,
        values: m.values,
        side: m.side,
        tierLabel: m.tierLabel,
      })),
    });
  }
  const flavour = cap?.flavour ?? entry.flavour;
  if (flavour) model.sections.push({ type: "flavour", text: flavour });
  return model;
}

function artFor(
  model: ItemPopupModel,
  cap: ItemViewCapture,
  entry: UniqueCatalogEntry | null,
  opts: BuildItemViewOptions,
): string | null {
  if (entry?.artRel) {
    return `/unique-icons/${entry.artRel}`;
  }
  return (
    resolveItemArt({
      kind: model.kind,
      name: model.name,
      typeLine: model.typeLine,
      baseManifest: opts.baseManifest,
      uniqueManifest: opts.uniqueManifest,
    })?.url ?? null
  );
}

/**
 * Build the shared ItemView for clipboard text and/or unique name match.
 */
export function buildItemView(
  source: ItemViewSource | string,
  opts: BuildItemViewOptions = {},
): ItemView {
  const src: ItemViewSource =
    typeof source === "string" ? { type: "clipboard", text: source } : source;

  let capture: ItemViewCapture;
  if (src.type === "clipboard") {
    capture = parseCapture(src.text);
  } else {
    capture = src.captureText
      ? parseCapture(src.captureText)
      : parseCapture(
          `Item Class: Unknown\nRarity: Unique\n${src.name}\n`,
        );
    if (!src.captureText) {
      capture = { ...capture, name: src.name, kind: "unique" };
    }
  }

  // Unique catalog match
  if (capture.kind === "unique") {
    const entry = lookupUnique(opts.uniqueCatalog, capture.name);
    if (entry) {
      const model = mergeUniqueWithCapture(entry, capture);
      return {
        model,
        artUrl: artFor(model, capture, entry, opts),
        uniqueMatched: true,
        uniqueKey: entry.name.toLowerCase(),
        capture,
      };
    }
  }

  // Magic / rare / normal / unmatched unique: full capture path
  const model = captureToModel(capture);
  return {
    model,
    artUrl: artFor(model, capture, null, opts),
    uniqueMatched: false,
    capture,
  };
}
