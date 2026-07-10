/**
 * Build an ItemPopupModel from raw PoE2 Ctrl+C clipboard text.
 * Structure follows in-game / poe2db blocks split by `--------`.
 */

import {
  emptyItemPopupModel,
  type ItemPopupKind,
  type ItemPopupModLine,
  type ItemPopupModel,
  type ItemPopupProperty,
  type ItemPopupSection,
} from "./model";

const SEPARATOR_RE = /^-{4,}$/;
const MARKERS = new Set(["Corrupted", "Mirrored", "Sanctified", "Unidentified"]);

/** Property keys that belong in the grey property block (not mods). */
const PROPERTY_PREFIXES = [
  "Quality",
  "Armour",
  "Evasion",
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
const FLAVOUR_RE = /^["“].*["”]$/s;
const HELP_HINTS =
  /place into|right click|can be|socket|panel to apply|skill gem/i;

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

function parseHeaderMeta(block: string[]): {
  itemClass: string | null;
  rarity: string;
  names: string[];
} {
  let itemClass: string | null = null;
  let rarity = "normal";
  const names: string[] = [];
  let pastMeta = false;
  for (const line of block) {
    if (line.startsWith("Item Class:")) {
      itemClass = line.slice("Item Class:".length).trim() || null;
      continue;
    }
    if (line.startsWith("Rarity:")) {
      rarity = line.slice("Rarity:".length).trim().toLowerCase() || "normal";
      pastMeta = true;
      continue;
    }
    if (pastMeta || (!line.includes(":") && !line.startsWith("Item Class"))) {
      if (!line.startsWith("Item Class:") && !line.startsWith("Rarity:")) {
        names.push(line);
      }
    }
  }
  // Fallback: lines after rarity missing — take non-meta lines
  if (names.length === 0) {
    for (const line of block) {
      if (line.startsWith("Item Class:") || line.startsWith("Rarity:")) continue;
      names.push(line);
    }
  }
  return { itemClass, rarity, names };
}

function detectKind(
  rarity: string,
  itemClass: string | null,
  blocks: string[][],
): ItemPopupKind {
  const cls = (itemClass ?? "").toLowerCase();
  if (
    cls.includes("gem") ||
    cls.includes("support") ||
    rarity === "gem" ||
    blocks.some((b) => b.some((l) => /^Support Requirements:/i.test(l) || /^Category:/i.test(l)))
  ) {
    return "gem";
  }
  if (
    cls.includes("currency") ||
    cls.includes("stackable") ||
    rarity === "currency" ||
    blocks.some((b) => b.some((l) => /^Stack Size:/i.test(l)))
  ) {
    return "currency";
  }
  if (rarity === "unique") return "unique";
  if (rarity === "rare") return "rare";
  if (rarity === "magic") return "magic";
  return "normal";
}

function isPropertyLine(line: string): boolean {
  if (REQ_LINE.test(line)) return false;
  if (MARKERS.has(line)) return false;
  if (line.startsWith("Item Level:")) return true;
  const colon = line.indexOf(":");
  if (colon > 0) {
    const label = line.slice(0, colon).trim();
    if (PROPERTY_PREFIXES.some((p) => label === p || label.startsWith(p))) return true;
  }
  // Class tag alone (e.g. "Bows", "Ezomyte Gloves", "Jewels") — short, no colon, first blocks
  if (!line.includes(":") && line.length < 40 && !/^[+\-0-9(]/.test(line) && !FLAVOUR_RE.test(line)) {
    // Could still be a mod — only treat as property when no leading +/number and no "to "
    if (!/\bto\b/i.test(line) && !/%/.test(line) && !line.includes("increased") && !line.includes("more")) {
      return true;
    }
  }
  return false;
}

function splitProperty(line: string): ItemPopupProperty {
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

/** Highlight roll numbers (standalone digits / ranges) for white spans. */
export function splitModValues(text: string): ItemPopupModLine {
  const values: string[] = [];
  // Capture numbers that look like rolls: 8, +3, 30-50, (30—50), 1.5, etc.
  const re = /(?<![\w])([+\-]?\d+(?:\.\d+)?(?:\s*[–—\-]\s*[+\-]?\d+(?:\.\d+)?)?|\(\d+(?:\.\d+)?[–—\-]\d+(?:\.\d+)?\))/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    values.push(m[1].replace(/\s+/g, ""));
  }
  return { text, values: values.length ? values : undefined };
}

function isFlavourBlock(lines: string[]): boolean {
  const joined = lines.join(" ").trim();
  if (FLAVOUR_RE.test(joined)) return true;
  // Multi-line flavour without closing quote on first line
  if (joined.startsWith('"') || joined.startsWith("“")) return true;
  return false;
}

function isHelpBlock(lines: string[]): boolean {
  const joined = lines.join(" ");
  return HELP_HINTS.test(joined);
}

function isSecDescrBlock(lines: string[], kind: ItemPopupKind): boolean {
  if (kind !== "gem") return false;
  // Gem secondary description: prose without leading +/number, not flavour/help
  const joined = lines.join(" ");
  if (isFlavourBlock(lines) || isHelpBlock(lines)) return false;
  if (/^[+\-(0-9]/.test(joined)) return false;
  if (lines.every((l) => isPropertyLine(l) || REQ_LINE.test(l))) return false;
  return lines.length >= 1 && joined.length > 40 && !joined.includes("% increased");
}

/**
 * Parse clipboard item text into a display model.
 */
export function itemPopupFromClipboard(text: string): ItemPopupModel {
  const blocks = splitBlocks(text);
  if (blocks.length === 0) {
    return emptyItemPopupModel({ name: text.trim() || "Item" });
  }

  const header = parseHeaderMeta(blocks[0]);
  const kind = detectKind(header.rarity, header.itemClass, blocks);
  const doubleLine = kind === "rare" || kind === "unique" || kind === "gem";

  const name = header.names[0] ?? "Item";
  // Second header line for double-line kinds (base / Support / etc.).
  const typeLine = doubleLine ? header.names[1] : undefined;

  const properties: ItemPopupProperty[] = [];
  const requirements: string[] = [];
  const sections: ItemPopupSection[] = [];
  const flags = {
    corrupted: false,
    mirrored: false,
    sanctified: false,
    unidentified: false,
  };

  // Walk body blocks (after header)
  for (let bi = 1; bi < blocks.length; bi++) {
    const block = blocks[bi];
    if (block.length === 0) continue;

    // Flag-only blocks
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
      sections.push({ type: "flavour", text: block.join("\n") });
      continue;
    }
    if (isHelpBlock(block)) {
      sections.push({ type: "help", text: block.join(" ") });
      continue;
    }
    if (isSecDescrBlock(block, kind)) {
      sections.push({ type: "secDescr", text: block.join(" ") });
      continue;
    }

    // Requirements block
    if (block.some((l) => REQ_LINE.test(l)) || block[0] === "Requirements:") {
      for (const l of block) {
        if (l === "Requirements:") continue;
        requirements.push(l.startsWith("Requires") || l.includes("Requirements") ? l : `Requires: ${l}`);
      }
      continue;
    }

    // Property block: all property-like lines
    const allProps = block.every(
      (l) => isPropertyLine(l) || l.startsWith("Item Level:") || /^Level:\s*\d+/.test(l),
    );
    if (allProps) {
      for (const l of block) {
        if (l.startsWith("Item Level:")) {
          properties.push(splitProperty(l));
          continue;
        }
        properties.push(splitProperty(l));
      }
      continue;
    }

    // Mixed or mod block
    const propLines: string[] = [];
    const modLines: string[] = [];
    for (const l of block) {
      if (MARKERS.has(l)) {
        if (l === "Corrupted") flags.corrupted = true;
        if (l === "Mirrored") flags.mirrored = true;
        if (l === "Sanctified") flags.sanctified = true;
        if (l === "Unidentified") flags.unidentified = true;
        continue;
      }
      if (isPropertyLine(l) && modLines.length === 0) {
        propLines.push(l);
      } else if (REQ_LINE.test(l)) {
        requirements.push(l);
      } else {
        modLines.push(l);
      }
    }
    for (const l of propLines) properties.push(splitProperty(l));
    if (modLines.length) {
      sections.push({
        type: "mods",
        lines: modLines.map(splitModValues),
      });
    }
  }

  return emptyItemPopupModel({
    kind,
    doubleLine,
    name,
    typeLine,
    properties,
    requirements,
    sections,
    flags,
  });
}
