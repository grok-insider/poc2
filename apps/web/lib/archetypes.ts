/// Build-archetype target presets, curated from current PoE2 (0.5) meta
/// research. Each preset is a full ~6-mod target (prefixes + suffixes) for a
/// given item class + attribute variant. Concepts are the engine's real
/// `ConceptId`s (incl. the derived `Spirit` / `SkillLevel`). Targets are
/// validated against the base's eligible pool before they're applied, so a
/// preset that lists a concept the base can't roll just drops that concept.
///
/// Mechanics that make these work (verified): a spec `{ concept, count: N,
/// allow_hybrid: false }` asks for N *distinct non-hybrid* mods of that concept
/// — e.g. `{ EnergyShield, count: 2, allow_hybrid: false }` ⇒ flat-ES + %-ES.

import { conceptPalette, bestTierMap, attributeVariant } from "./concepts";
import type { EligibleModsResponse, TargetSpec } from "./types";

export type AttrPool = "str" | "dex" | "int" | "hybrid";

export interface Archetype {
  id: string;
  /** Short chip label, e.g. "Max ES". */
  name: string;
  description: string;
  /** Item class ids this applies to (empty = any class). */
  classes: string[];
  /** Attribute variants this applies to (empty = any / non-attribute base). */
  pools: AttrPool[];
  prefixes: TargetSpec[];
  suffixes: TargetSpec[];
}

// ---- spec builders ----------------------------------------------------------

const p = (concept: string, count = 1, allow_hybrid = true): TargetSpec => ({
  concept,
  affix: "prefix",
  count,
  min_tier: 1,
  allow_hybrid,
});
const s = (concept: string, count = 1, allow_hybrid = true): TargetSpec => ({
  concept,
  affix: "suffix",
  count,
  min_tier: 1,
  allow_hybrid,
});
const pAny = (concept_any: string[], count = 1): TargetSpec => ({
  concept_any,
  affix: "prefix",
  count,
  min_tier: 1,
  allow_hybrid: true,
});
const sAny = (concept_any: string[], count = 1): TargetSpec => ({
  concept_any,
  affix: "suffix",
  count,
  min_tier: 1,
  allow_hybrid: true,
});

const RES = ["FireResistance", "ColdResistance", "LightningResistance", "AllResistances"];
const ELE_ADDED = ["AddedFireDamage", "AddedColdDamage", "AddedLightningDamage"];

// ---- the curated presets ----------------------------------------------------

export const ARCHETYPES: Archetype[] = [
  // ===== Body armour =====
  {
    id: "ba-int-ci",
    name: "Max ES (CI)",
    description: "Pure energy-shield stacker — flat + % ES, no hybrid; tri-res.",
    classes: ["BodyArmour"],
    pools: ["int"],
    prefixes: [p("EnergyShield", 2, false), p("IncreasedSpellDamage")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "ba-int-minion",
    name: "Minion (ES + Spirit)",
    description: "Two ES tiers (flat + %, no hybrid) plus a Spirit prefix for minions.",
    classes: ["BodyArmour"],
    pools: ["int"],
    prefixes: [p("EnergyShield", 2, false), p("Spirit")],
    suffixes: [sAny(RES, 2), s("AllAttributes")],
  },
  {
    id: "ba-int-caster",
    name: "Caster",
    description: "ES + spell damage + mana, with resistances.",
    classes: ["BodyArmour"],
    pools: ["int"],
    prefixes: [p("EnergyShield"), p("IncreasedSpellDamage"), p("Mana")],
    suffixes: [s("CastSpeed"), sAny(RES, 2)],
  },
  {
    id: "ba-str-life",
    name: "Armour + Life",
    description: "Physical defence: armour + life, tri-res.",
    classes: ["BodyArmour"],
    pools: ["str"],
    prefixes: [p("Armour"), p("Life")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "ba-dex-life",
    name: "Evasion + Life",
    description: "Evasion + life, tri-res.",
    classes: ["BodyArmour"],
    pools: ["dex"],
    prefixes: [p("Evasion"), p("Life")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "ba-hybrid-life",
    name: "Hybrid defence",
    description: "Two defence layers + life, tri-res.",
    classes: ["BodyArmour"],
    pools: ["hybrid"],
    prefixes: [pAny(["Armour", "Evasion", "EnergyShield"], 2), p("Life")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },

  // ===== Helmet / Gloves / Boots (by attribute) =====
  {
    id: "armour-int-def",
    name: "ES + res",
    description: "Energy shield + life, resistances.",
    classes: ["Helmet", "Gloves", "Boots"],
    pools: ["int"],
    prefixes: [p("EnergyShield"), pAny(["Life", "Mana"])],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "armour-str-def",
    name: "Armour + Life",
    description: "Armour + life, resistances.",
    classes: ["Helmet", "Gloves", "Boots"],
    pools: ["str"],
    prefixes: [p("Armour"), p("Life")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "armour-dex-def",
    name: "Evasion + Life",
    description: "Evasion + life, resistances.",
    classes: ["Helmet", "Gloves", "Boots"],
    pools: ["dex"],
    prefixes: [p("Evasion"), p("Life")],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "boots-ms",
    name: "Movement + res",
    description: "Boots: movement speed is king; life + tri-res.",
    classes: ["Boots"],
    pools: ["str", "dex", "int", "hybrid"],
    prefixes: [pAny(["Armour", "Evasion", "EnergyShield"]), p("Life")],
    suffixes: [s("MovementSpeed"), sAny(RES, 2)],
  },
  {
    id: "gloves-atk",
    name: "Attack speed",
    description: "Gloves: attack speed + life + res.",
    classes: ["Gloves"],
    pools: ["str", "dex", "int", "hybrid"],
    prefixes: [pAny(["Armour", "Evasion", "EnergyShield"]), p("Life")],
    suffixes: [s("AttackSpeed"), sAny(RES, 2)],
  },

  // ===== Foci (caster off-hand, int) =====
  {
    id: "focus-es-caster",
    name: "ES caster",
    description: "Two ES tiers + spell power; cast speed + crit.",
    classes: ["Focus"],
    pools: ["int"],
    prefixes: [p("EnergyShield", 2, false), pAny(["IncreasedSpellDamage", "SkillLevel"])],
    suffixes: [s("CastSpeed"), s("CritChance"), sAny(RES)],
  },
  {
    id: "focus-minion",
    name: "Minion focus",
    description: "ES + +minion/skill levels + Spirit.",
    classes: ["Focus"],
    pools: ["int"],
    prefixes: [p("EnergyShield"), p("SkillLevel"), p("Spirit")],
    suffixes: [s("CastSpeed"), sAny(RES, 2)],
  },

  // ===== Caster weapons (Staff / Wand / Sceptre) =====
  {
    id: "caster-spell",
    name: "Spell damage",
    description: "Spell damage + skill levels; cast speed + crit.",
    classes: ["Staff", "Warstaff", "Wand", "Sceptre"],
    pools: [],
    prefixes: [p("IncreasedSpellDamage"), p("SkillLevel"), pAny(["IncreasedElementalDamage", "Mana"])],
    suffixes: [s("CastSpeed"), s("CritChance"), sAny([...RES, "Mana"])],
  },
  {
    id: "caster-minion",
    name: "Minion",
    description: "+Skill levels + minion damage + Spirit.",
    classes: ["Sceptre", "Wand", "Staff"],
    pools: [],
    prefixes: [p("SkillLevel"), p("MinionDamage"), p("Spirit")],
    suffixes: [s("CastSpeed"), sAny(RES, 2)],
  },

  // ===== Martial weapons (Quarterstaff + 1H/2H) =====
  {
    id: "wpn-phys",
    name: "Phys attack",
    description: "Two added-phys tiers (no hybrid) + skill levels; crit + attack speed.",
    classes: [
      "Quarterstaff",
      "OneHandMace",
      "TwoHandMace",
      "OneHandSword",
      "TwoHandSword",
      "OneHandAxe",
      "TwoHandAxe",
      "Spear",
      "Flail",
    ],
    pools: [],
    prefixes: [p("AddedPhysicalDamage", 2, false), p("SkillLevel")],
    suffixes: [s("CritChance"), s("CritDamage"), s("AttackSpeed")],
  },
  {
    id: "wpn-ele",
    name: "Elemental attack",
    description: "Increased phys + added elemental + skill levels; crit + attack speed.",
    classes: [
      "Quarterstaff",
      "OneHandMace",
      "TwoHandMace",
      "OneHandSword",
      "TwoHandSword",
      "OneHandAxe",
      "TwoHandAxe",
      "Spear",
      "Flail",
      "Bow",
      "Crossbow",
    ],
    pools: [],
    prefixes: [p("IncreasedPhysicalDamage"), pAny(ELE_ADDED), p("SkillLevel")],
    suffixes: [s("CritChance"), s("CritDamage"), s("AttackSpeed")],
  },

  // ===== Jewellery =====
  {
    id: "ring-ele",
    name: "Elemental attack",
    description: "Stacked flat elemental damage + resistances.",
    classes: ["Ring"],
    pools: [],
    prefixes: [pAny(ELE_ADDED, 2)],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "ring-def",
    name: "Life / ES + res",
    description: "Life or ES + attributes; resistances.",
    classes: ["Ring"],
    pools: [],
    prefixes: [pAny(["Life", "EnergyShield"]), pAny(["AllAttributes", "Mana"])],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "amulet-gem",
    name: "+Skill levels",
    description: "Gem levels + life/ES; resistances + attributes.",
    classes: ["Amulet"],
    pools: [],
    prefixes: [p("SkillLevel"), pAny(["Life", "EnergyShield"]), pAny(["Spirit", "AllAttributes"])],
    suffixes: [sAny(RES, 2), s("AllAttributes")],
  },
  {
    id: "belt-life",
    name: "Life + res",
    description: "Life + defence; resistances.",
    classes: ["Belt"],
    pools: [],
    prefixes: [p("Life"), pAny(["Armour", "Evasion", "EnergyShield"])],
    suffixes: [sAny(RES, 2), s("ChaosResistance")],
  },
  {
    id: "quiver-atk",
    name: "Attack damage",
    description: "Stacked added damage; crit + attack speed.",
    classes: ["Quiver"],
    pools: [],
    prefixes: [pAny(["AddedPhysicalDamage", ...ELE_ADDED], 2)],
    suffixes: [s("CritChance"), s("CritDamage"), s("AttackSpeed")],
  },

  // ===== Shields =====
  {
    id: "shield-es",
    name: "ES + res",
    description: "Two ES tiers + resistances; block.",
    classes: ["Shield"],
    pools: ["int", "hybrid"],
    prefixes: [p("EnergyShield", 2, false), pAny(["Life", "Armour"])],
    suffixes: [sAny(RES, 2), s("Block")],
  },
  {
    id: "shield-armour",
    name: "Armour + Life",
    description: "Armour + life; resistances + block.",
    classes: ["Shield"],
    pools: ["str", "dex"],
    prefixes: [pAny(["Armour", "Evasion"]), p("Life")],
    suffixes: [sAny(RES, 2), s("Block")],
  },
];

// ---- matching + validation --------------------------------------------------

/** Archetypes that apply to the current item (class + attribute variant). */
export function applicableArchetypes(eligible: EligibleModsResponse | null): Archetype[] {
  if (!eligible || eligible.data_available === false) return [];
  const cls = eligible.item_class;
  const pool = attributeVariant(eligible);
  return ARCHETYPES.filter(
    (a) =>
      (a.classes.length === 0 || a.classes.includes(cls)) &&
      (a.pools.length === 0 || (pool != null && a.pools.includes(pool))),
  );
}

/**
 * Validate a preset against the base's real pool: drop concepts the base can't
 * roll, and clamp each spec's `min_tier` to the best tier reachable at the
 * item's ilvl. Returns the prefix/suffix specs ready for `setGoal`.
 */
export function validateArchetype(
  a: Archetype,
  eligible: EligibleModsResponse | null,
): { prefixes: TargetSpec[]; suffixes: TargetSpec[] } {
  const palette = conceptPalette(eligible);
  const best = bestTierMap(eligible);
  const prefSet = new Set(palette.prefixes.map((o) => o.concept));
  const sufSet = new Set(palette.suffixes.map((o) => o.concept));

  const prefixes: TargetSpec[] = [];
  const suffixes: TargetSpec[] = [];

  // Place each spec in the affix the base ACTUALLY rolls its concept(s) on —
  // a concept may be a prefix on one base and a suffix on another (e.g.
  // +skill-levels). Prefer the archetype's intended affix when available.
  const place = (spec: TargetSpec, intended: "prefix" | "suffix") => {
    const concepts = spec.concept != null ? [spec.concept] : spec.concept_any ?? [];
    const inPref = concepts.filter((c) => prefSet.has(c));
    const inSuf = concepts.filter((c) => sufSet.has(c));

    let affix: "prefix" | "suffix";
    let kept: string[];
    if (intended === "prefix" && inPref.length) [affix, kept] = ["prefix", inPref];
    else if (intended === "suffix" && inSuf.length) [affix, kept] = ["suffix", inSuf];
    else if (inPref.length) [affix, kept] = ["prefix", inPref];
    else if (inSuf.length) [affix, kept] = ["suffix", inSuf];
    else return; // base can't roll any of these concepts

    const tier = Math.min(...kept.map((c) => best.get(`${affix}:${c}`) ?? 1));
    const out: TargetSpec =
      spec.concept != null
        ? { ...spec, affix, min_tier: tier }
        : { ...spec, concept_any: kept, affix, min_tier: tier };
    (affix === "prefix" ? prefixes : suffixes).push(out);
  };

  a.prefixes.forEach((s) => place(s, "prefix"));
  a.suffixes.forEach((s) => place(s, "suffix"));
  return { prefixes, suffixes };
}
