/// Vendor / stash shopping-filter generator: hand-authored micro-patterns
/// against the item tooltip lines the in-game search scans, e.g.
///   "Item Class: Body Armours" · "Rarity: Rare" · "Item Level: 79"
///   "Quality: +20%" · "Sockets: S S" · "Requires: Level 60"
///   "+85 to maximum Life" · "30% increased Movement Speed"
///
/// Every pattern is authored from those line shapes (clean-room; the
/// category concept mirrors community tools like poe2.re, which is
/// unlicensed reference-only).

import { exactAlternation, rangeRegex } from "./numberRegex";
import type { SearchTerm } from "./searchString";

/** PoE2 `Item Class:` display names the class filter can target. */
export const VENDOR_CLASSES = [
  "Amulets",
  "Belts",
  "Body Armours",
  "Boots",
  "Bows",
  "Bucklers",
  "Charms",
  "Claws",
  "Crossbows",
  "Daggers",
  "Flails",
  "Foci",
  "Gloves",
  "Helmets",
  "Jewels",
  "One Hand Axes",
  "One Hand Maces",
  "One Hand Swords",
  "Quarterstaves",
  "Quivers",
  "Rings",
  "Sceptres",
  "Shields",
  "Spears",
  "Staves",
  "Talismans",
  "Two Hand Axes",
  "Two Hand Maces",
  "Two Hand Swords",
  "Wands",
  "Warstaves",
] as const;

export type VendorClass = (typeof VENDOR_CLASSES)[number];

/** Shortest lowercase prefix of `name` unique among all class names. */
export function classPrefix(name: VendorClass): string {
  const lower = name.toLowerCase();
  const others = VENDOR_CLASSES.filter((c) => c !== name).map((c) => c.toLowerCase());
  for (let len = 1; len <= lower.length; len++) {
    const prefix = lower.slice(0, len);
    if (!others.some((o) => o.startsWith(prefix))) return prefix;
  }
  return lower;
}

export interface VendorSettings {
  classes: VendorClass[];
  rarity: { normal: boolean; magic: boolean; rare: boolean };
  itemLevel: { min: number; max: number };
  requiresLevel: { min: number; max: number };
  quality: boolean;
  sockets: boolean;
  movementSpeeds: number[]; // e.g. [30, 25]
  resists: { fire: boolean; cold: boolean; lightning: boolean; chaos: boolean };
  attributes: { strength: boolean; dexterity: boolean; intelligence: boolean; all: boolean };
  mods: {
    life: boolean;
    mana: boolean;
    spirit: boolean;
    rarity: boolean;
    physicalDamage: boolean;
    spellDamage: boolean;
    attackSpeed: boolean;
    castSpeed: boolean;
    addsFire: boolean;
    addsCold: boolean;
    addsLightning: boolean;
    addsChaos: boolean;
    skillsAll: boolean;
    skillsMinion: boolean;
    skillsMelee: boolean;
    skillsProjectile: boolean;
    skillsSpell: boolean;
  };
}

export function emptyVendorSettings(): VendorSettings {
  return {
    classes: [],
    rarity: { normal: false, magic: false, rare: false },
    itemLevel: { min: 0, max: 0 },
    requiresLevel: { min: 0, max: 0 },
    quality: false,
    sockets: false,
    movementSpeeds: [],
    resists: { fire: false, cold: false, lightning: false, chaos: false },
    attributes: { strength: false, dexterity: false, intelligence: false, all: false },
    mods: {
      life: false,
      mana: false,
      spirit: false,
      rarity: false,
      physicalDamage: false,
      spellDamage: false,
      attackSpeed: false,
      castSpeed: false,
      addsFire: false,
      addsCold: false,
      addsLightning: false,
      addsChaos: false,
      skillsAll: false,
      skillsMinion: false,
      skillsMelee: false,
      skillsProjectile: false,
      skillsSpell: false,
    },
  };
}

function classTerm(classes: VendorClass[]): string | null {
  if (classes.length === 0) return null;
  const prefixes = classes.map(classPrefix);
  // "s: " anchors on the tail of "Item Class: ".
  return prefixes.length === 1 ? `s: ${prefixes[0]}` : `s: (${prefixes.join("|")})`;
}

function rarityTerm(r: VendorSettings["rarity"]): string | null {
  // "y: " anchors on the tail of "Rarity: "; first letter disambiguates.
  const letters = [r.rare ? "r" : null, r.magic ? "m" : null, r.normal ? "n" : null].filter(
    (x): x is string => x !== null,
  );
  if (letters.length === 0 || letters.length === 3) return null;
  return letters.length === 1 ? `y: ${letters[0]}` : `y: (${letters.join("|")})`;
}

function movementTerm(speeds: number[]): string | null {
  if (speeds.length === 0) return null;
  const alt = exactAlternation(speeds);
  if (alt === "") return null;
  // "#% increased Movement Speed" — "mov" only occurs in movement lines.
  return `${alt}% i.*mov`;
}

function resistTerm(r: VendorSettings["resists"]): string | null {
  const frags = [
    r.fire ? "fi" : null,
    r.cold ? "co" : null,
    r.lightning ? "li" : null,
    r.chaos ? "ch" : null,
  ].filter((x): x is string => x !== null);
  if (frags.length === 0) return null;
  if (frags.length === 4) return "resist";
  // "+#% to Fire Resistance"
  return frags.length === 1 ? `${frags[0]}.*resi` : `(${frags.join("|")}).*resi`;
}

function attributeTerm(a: VendorSettings["attributes"]): string | null {
  // "+# to Strength / Dexterity / Intelligence / all Attributes".
  // "all a" (not "all") — "to All Elemental Resistances" also contains "o all".
  const frags = [
    a.strength ? "str" : null,
    a.dexterity ? "dex" : null,
    a.intelligence ? "int" : null,
    a.all ? "all a" : null,
  ].filter((x): x is string => x !== null);
  if (frags.length === 0) return null;
  return frags.length === 1 ? `o ${frags[0]}` : `o (${frags.join("|")})`;
}

function modTerms(m: VendorSettings["mods"]): string[] {
  const adds = [
    m.addsFire ? "fi" : null,
    m.addsCold ? "co" : null,
    m.addsLightning ? "li" : null,
    m.addsChaos ? "ch" : null,
  ].filter((x): x is string => x !== null);
  // "Adds # to # Fire Damage" — digit, space, element, later "Damage".
  const addsTerm =
    adds.length === 0
      ? null
      : adds.length === 1
        ? `\\d ${adds[0]}.*da`
        : `\\d (${adds.join("|")}).*da`;

  return [
    m.life ? "m life" : null, // "+# to maximum Life"
    m.mana ? "m mana" : null, // "+# to maximum Mana"
    m.spirit ? "spiri" : null, // "+# to Spirit"
    m.rarity ? "d rari" : null, // "#% increased Rarity of Items found" (not "Rarity: …")
    m.physicalDamage ? "ph.*da" : null, // "#% increased Physical Damage"
    m.spellDamage ? "ll da" : null, // "#% increased Spell Damage"
    m.attackSpeed ? "k spe" : null, // "#% increased Attack Speed"
    m.castSpeed ? "st spe" : null, // "#% increased Cast Speed"
    addsTerm,
    m.skillsAll ? "l of all" : null, // "+# to Level of all … Skills"
    m.skillsMinion ? "ion sk" : null, // "… Minion Skills"
    m.skillsMelee ? "ee sk" : null, // "… Melee Skills"
    m.skillsProjectile ? "le sk" : null, // "… Projectile Skills"
    m.skillsSpell ? "ll sp" : null, // "… Spell Skills" (incl. elemental)
  ].filter((x): x is string => x !== null);
}

/** Generate the vendor tab's terms (AND-combined by the assembler). */
export function vendorTerms(s: VendorSettings): SearchTerm[] {
  const ilvl = rangeRegex(s.itemLevel.min, s.itemLevel.max);
  const reqLevel = rangeRegex(s.requiresLevel.min, s.requiresLevel.max);

  const patterns = [
    classTerm(s.classes),
    rarityTerm(s.rarity),
    // "Item Level: 79" / "Requires: Level 60" — the prefix pins the match
    // to the line's number; the trailing \b stops partial matches like a
    // [5-9] arm matching the "5" of "59".
    ilvl !== "" ? `m level: ${ilvl}\\b` : null,
    reqLevel !== "" ? `s: level ${reqLevel}\\b` : null,
    s.quality ? "y: \\+" : null, // "Quality: +20%"
    s.sockets ? "ts: s" : null, // "Sockets: S"
    movementTerm(s.movementSpeeds),
    resistTerm(s.resists),
    attributeTerm(s.attributes),
    ...modTerms(s.mods),
  ].filter((x): x is string => x !== null && x !== "");

  return patterns.map((pattern) => ({ pattern }));
}
