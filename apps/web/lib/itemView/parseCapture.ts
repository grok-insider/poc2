/**
 * Parse PoE2 clipboard text (basic or advanced mod format) into ItemViewCapture.
 */

import { splitModValues } from "@/lib/itemPopup/fromClipboard";
import type { ItemPopupKind } from "@/lib/itemPopup/model";
import type { ItemViewCapture, ItemViewMod } from "./types";

const SEPARATOR_RE = /^-{4,}$/;
const MARKERS = new Set(["Corrupted", "Mirrored", "Sanctified", "Unidentified"]);

const PROPERTY_PREFIXES = [
  "Quality",
  "Armour",
  "Evasion",
  "Evasion Rating",
  "Energy Shield",
  "Block",
  "Physical Damage",
  "Elemental Damage",
  "Chaos Damage",
  "Critical Hit Chance",
  "Critical Strike Chance",
  "Attacks per Second",
  "Weapon Range",
  "Item Level",
  "Sockets",
  "Rune Sockets",
  "Stack Size",
  "Limited To",
  "Limited to",
  "Radius",
  "Reload Time",
  "Charm Slots",
  "Duration",
  "Charges",
  "Category",
  "Level",
  "Experience",
];

const REQ_LINE = /^(Requires|Requirements|Support Requirements|Skill Requirements)\b/i;
const ADV_HEADER =
  /^\{\s*(Prefix|Suffix|Implicit|Unique|Desecrated|Crafted|Corruption|Rune|Enchantment)?\s*(Prefix|Suffix|Implicit|Unique)?\s*(Modifier|Enhancement)?[^}]*\}$/i;
const TIER_RE = /\(Tier:\s*(\d+)\)/i;
const FLAVOUR_START = /^["“]/;
const GRANTS_SKILL = /^Grants Skill:/i;

function splitBlocks(text: string): string[][] {
  const lines = text.split(/\r?\n/).map((l) => l.trimEnd());
  const blocks: string[][] = [];
  let cur: string[] = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (SEPARATOR_RE.test(line)) {
      if (cur.length) blocks.push(cur);
      cur = [];
      continue;
    }
    if (line.length) cur.push(line);
  }
  if (cur.length) blocks.push(cur);
  return blocks;
}

function isPropertyLine(line: string): boolean {
  if (REQ_LINE.test(line) || MARKERS.has(line) || GRANTS_SKILL.test(line)) return false;
  if (line.startsWith("Item Level:") || line.startsWith("Sockets:")) return true;
  const colon = line.indexOf(":");
  if (colon > 0) {
    const label = line.slice(0, colon).trim();
    if (PROPERTY_PREFIXES.some((p) => label === p || label.startsWith(p))) return true;
  }
  // Class tag alone
  if (
    !line.includes(":") &&
    line.length < 48 &&
    !/^[+\-0-9(]/.test(line) &&
    !FLAVOUR_START.test(line) &&
    !line.includes("%") &&
    !/\bto\b/i.test(line) &&
    !line.includes("increased") &&
    !line.includes("more") &&
    !line.endsWith("(rune)") &&
    !line.startsWith("{")
  ) {
    return true;
  }
  return false;
}

function splitProperty(line: string): { label?: string; value?: string; text?: string } {
  const colon = line.indexOf(":");
  if (colon > 0) {
    return {
      label: line.slice(0, colon).trim(),
      value: line.slice(colon + 1).trim(),
      text: line,
    };
  }
  return { text: line };
}

function detectKind(rarity: string, itemClass: string | null, lines: string[]): ItemPopupKind {
  const cls = (itemClass ?? "").toLowerCase();
  if (
    cls.includes("gem") ||
    cls.includes("support") ||
    rarity === "gem" ||
    lines.some((l) => /^Support Requirements:/i.test(l) || /^Category:/i.test(l))
  ) {
    return "gem";
  }
  if (
    cls.includes("currency") ||
    cls.includes("stackable") ||
    rarity === "currency" ||
    lines.some((l) => /^Stack Size:/i.test(l))
  ) {
    return "currency";
  }
  if (rarity === "unique") return "unique";
  if (rarity === "rare") return "rare";
  if (rarity === "magic") return "magic";
  return "normal";
}

function parseAdvancedHeader(header: string): Partial<ItemViewMod> {
  const h = header.replace(/^\{|\}$/g, "").trim();
  const tierM = h.match(TIER_RE);
  const tierLabel = tierM ? `T${tierM[1]}` : undefined;
  let side: ItemViewMod["side"];
  const lower = h.toLowerCase();
  if (lower.includes("corruption")) side = "corruption";
  else if (lower.includes("desecrated") && lower.includes("prefix")) side = "desecrated";
  else if (lower.includes("desecrated") && lower.includes("suffix")) side = "desecrated";
  else if (lower.includes("crafted")) side = "crafted";
  else if (lower.includes("unique")) side = "unique";
  else if (lower.includes("implicit")) side = "implicit";
  else if (lower.includes("prefix")) side = "prefix";
  else if (lower.includes("suffix")) side = "suffix";
  else if (lower.includes("rune")) side = "rune";
  else if (lower.includes("enchant")) side = "enchant";

  // Side labels for display
  if (side === "desecrated") {
    if (lower.includes("prefix")) side = "prefix";
    else if (lower.includes("suffix")) side = "suffix";
  }
  if (side === "crafted") {
    if (lower.includes("prefix")) side = "prefix";
    else if (lower.includes("suffix")) side = "suffix";
  }

  let displayTier = tierLabel;
  if (side === "unique") displayTier = displayTier ?? "U";
  if (side === "implicit") displayTier = displayTier ?? "I";
  if (side === "corruption") displayTier = displayTier ?? "E";

  return { side, tierLabel: displayTier, advancedHeader: header };
}

function isFlavourBlock(lines: string[]): boolean {
  const joined = lines.join(" ").trim();
  return FLAVOUR_START.test(joined);
}

/**
 * Parse clipboard into a structured capture for item presentation.
 */
export function parseCapture(text: string): ItemViewCapture {
  const rawText = text;
  const blocks = splitBlocks(text);
  const allLines = text.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);

  let itemClass: string | null = null;
  let rarity = "normal";
  const names: string[] = [];

  if (blocks.length === 0) {
    return {
      kind: "normal",
      itemClass: null,
      rarity: "normal",
      name: text.trim() || "Item",
      properties: [],
      requirements: [],
      mods: [],
      flags: { corrupted: false, mirrored: false, sanctified: false, unidentified: false },
      advanced: false,
      rawText,
    };
  }

  // Header block
  const header = blocks[0];
  let pastRarity = false;
  for (const line of header) {
    if (line.startsWith("Item Class:")) {
      itemClass = line.slice("Item Class:".length).trim() || null;
      continue;
    }
    if (line.startsWith("Rarity:")) {
      rarity = line.slice("Rarity:".length).trim().toLowerCase() || "normal";
      pastRarity = true;
      continue;
    }
    if (pastRarity) names.push(line);
  }
  if (names.length === 0) {
    for (const line of header) {
      if (!line.startsWith("Item Class:") && !line.startsWith("Rarity:")) names.push(line);
    }
  }

  const kind = detectKind(rarity, itemClass, allLines);
  const doubleLike = kind === "rare" || kind === "unique" || kind === "gem";
  const name = names[0] ?? "Item";
  const typeLine = doubleLike ? names[1] : undefined;

  const properties: ItemViewCapture["properties"] = [];
  const requirements: string[] = [];
  const mods: ItemViewMod[] = [];
  const flags = {
    corrupted: false,
    mirrored: false,
    sanctified: false,
    unidentified: false,
  };
  let sockets: string | undefined;
  let itemLevel: number | null = null;
  let grantsSkill: string | undefined;
  let flavour: string | undefined;
  let help: string | undefined;
  let advanced = false;
  let pendingAdv: Partial<ItemViewMod> | null = null;

  const pushMod = (line: string, extra?: Partial<ItemViewMod>) => {
    const base = splitModValues(line);
    const side =
      extra?.side ??
      (line.endsWith("(rune)") ? "rune" : pendingAdv?.side);
    const tierLabel =
      extra?.tierLabel ??
      (side === "rune" ? undefined : pendingAdv?.tierLabel);
    mods.push({
      text: base.text,
      values: base.values,
      side,
      tierLabel,
      advancedHeader: extra?.advancedHeader ?? pendingAdv?.advancedHeader,
    });
    pendingAdv = null;
  };

  for (let bi = 1; bi < blocks.length; bi++) {
    const block = blocks[bi];
    if (block.every((l) => MARKERS.has(l))) {
      for (const l of block) {
        if (l === "Corrupted") flags.corrupted = true;
        if (l === "Mirrored") flags.mirrored = true;
        if (l === "Sanctified") flags.sanctified = true;
        if (l === "Unidentified") flags.unidentified = true;
      }
      continue;
    }

    if (isFlavourBlock(block)) {
      flavour = block.join("\n");
      continue;
    }

    const joined = block.join(" ");
    if (/place into|right click|can be desecrated/i.test(joined) && !/^[+\-(0-9]/.test(joined)) {
      help = joined;
      continue;
    }

    if (block.some((l) => REQ_LINE.test(l)) || block[0] === "Requirements:") {
      for (const l of block) {
        if (l === "Requirements:") continue;
        if (REQ_LINE.test(l) || l.match(/^(Level|Str|Dex|Int)/i)) {
          requirements.push(
            l.startsWith("Requires") || l.startsWith("Requirements")
              ? l
              : requirements.length === 0
                ? `Requires: ${l}`
                : l,
          );
        }
      }
      // Collapse multi-line reqs into one
      if (requirements.length > 1 && !requirements[0].includes(",")) {
        const first = requirements[0];
        const rest = requirements.slice(1).join(", ");
        requirements.length = 0;
        requirements.push(`${first}${first.endsWith(":") ? " " : ", "}${rest}`);
      }
      continue;
    }

    // Property-only block
    if (
      block.every(
        (l) =>
          isPropertyLine(l) ||
          l.startsWith("Item Level:") ||
          l.startsWith("Sockets:") ||
          /^Level:\s*\d+/.test(l),
      )
    ) {
      for (const l of block) {
        if (l.startsWith("Sockets:")) {
          sockets = l.slice("Sockets:".length).trim();
          properties.push(splitProperty(l));
          continue;
        }
        if (l.startsWith("Item Level:")) {
          const m = l.match(/(\d+)/);
          itemLevel = m ? Number(m[1]) : null;
          properties.push(splitProperty(l));
          continue;
        }
        properties.push(splitProperty(l));
      }
      continue;
    }

    // Mod / advanced / mixed block
    for (const l of block) {
      if (MARKERS.has(l)) {
        if (l === "Corrupted") flags.corrupted = true;
        if (l === "Mirrored") flags.mirrored = true;
        if (l === "Sanctified") flags.sanctified = true;
        if (l === "Unidentified") flags.unidentified = true;
        continue;
      }
      if (l.startsWith("{") && l.endsWith("}")) {
        advanced = true;
        pendingAdv = parseAdvancedHeader(l);
        continue;
      }
      if (GRANTS_SKILL.test(l)) {
        grantsSkill = l;
        continue;
      }
      if (isPropertyLine(l) && mods.length === 0 && !pendingAdv) {
        if (l.startsWith("Sockets:")) sockets = l.slice("Sockets:".length).trim();
        if (l.startsWith("Item Level:")) {
          const m = l.match(/(\d+)/);
          itemLevel = m ? Number(m[1]) : null;
        }
        properties.push(splitProperty(l));
        continue;
      }
      if (REQ_LINE.test(l)) {
        requirements.push(l);
        continue;
      }
      pushMod(l);
    }
  }

  // Inject class as first property when not present
  if (itemClass && !properties.some((p) => p.text === itemClass || p.label === "Item Class")) {
    // "Body Armours" style from Item Class line
    properties.unshift({ text: itemClass });
  }

  return {
    kind,
    itemClass,
    rarity,
    name,
    typeLine,
    properties,
    requirements,
    sockets,
    itemLevel,
    mods,
    grantsSkill,
    flavour,
    help,
    flags,
    advanced,
    rawText,
  };
}
