import { humanizeModId, modText } from "@/lib/format";
import {
  assembleSearch,
  MAX_SEARCH_LENGTH,
  type SearchTerm,
} from "@/lib/regex/searchString";
import { termsForMods, viewLines } from "@/lib/regex/modTerms";
import {
  emptyVendorSettings,
  VENDOR_CLASSES,
  vendorTerms,
  type VendorClass,
  type VendorSettings,
} from "@/lib/regex/vendor";
import type { HyprOverlayPayload } from "@/lib/desktop";
import type { EligibleModView } from "@/lib/types";

export interface RegexOverlayState {
  activeTab: string;
  focusIndex: number;
  selected: string[];
}

export interface RegexOverlayData {
  itemMods?: EligibleModView[] | null;
  waystoneMods?: EligibleModView[] | null;
  tabletMods?: EligibleModView[] | null;
}

interface RegexControl {
  id: string;
  tab: string;
  label: string;
  detail?: string;
  disabled?: boolean;
  vendor?: (settings: VendorSettings) => void;
  term?: SearchTerm;
  pool?: keyof RegexOverlayData;
  modId?: string;
}

const TABS = [
  { id: "items", label: "Items" },
  { id: "props", label: "Props" },
  { id: "mods", label: "Mods" },
  { id: "maps", label: "Maps" },
  { id: "tablet", label: "Tablets" },
] as const;

const POOL_LIMITS: Record<keyof RegexOverlayData, number> = {
  itemMods: 9,
  waystoneMods: 9,
  tabletMods: 8,
};

const DIRECT = (pattern: string): SearchTerm => ({ pattern });

const STATIC_CONTROLS: RegexControl[] = [
  { id: "rare", tab: "items", label: "Rare", vendor: (s) => (s.rarity.rare = true) },
  { id: "magic", tab: "items", label: "Magic", vendor: (s) => (s.rarity.magic = true) },
  { id: "normal", tab: "items", label: "Normal", vendor: (s) => (s.rarity.normal = true) },
  { id: "quality", tab: "items", label: "Quality", detail: "Quality: +%", vendor: (s) => (s.quality = true) },
  { id: "sockets", tab: "items", label: "Sockets", detail: "Sockets: S", vendor: (s) => (s.sockets = true) },
  ...(["Body Armours", "Boots", "Rings", "Jewels"] as VendorClass[]).map((klass) => ({
    id: `class:${klass}`,
    tab: "items",
    label: klass,
    vendor: (s: VendorSettings) => {
      s.classes.push(klass);
    },
  })),

  { id: "move30", tab: "props", label: "30% movement", vendor: (s) => s.movementSpeeds.push(30) },
  { id: "move35", tab: "props", label: "35% movement", vendor: (s) => s.movementSpeeds.push(35) },
  { id: "fireRes", tab: "props", label: "Fire resistance", vendor: (s) => (s.resists.fire = true) },
  { id: "coldRes", tab: "props", label: "Cold resistance", vendor: (s) => (s.resists.cold = true) },
  { id: "lightningRes", tab: "props", label: "Lightning resistance", vendor: (s) => (s.resists.lightning = true) },
  { id: "chaosRes", tab: "props", label: "Chaos resistance", vendor: (s) => (s.resists.chaos = true) },
  { id: "allAttributes", tab: "props", label: "All attributes", vendor: (s) => (s.attributes.all = true) },

  { id: "life", tab: "mods", label: "Maximum life", vendor: (s) => (s.mods.life = true) },
  { id: "mana", tab: "mods", label: "Maximum mana", vendor: (s) => (s.mods.mana = true) },
  { id: "spirit", tab: "mods", label: "Spirit", vendor: (s) => (s.mods.spirit = true) },
  { id: "rarityMod", tab: "mods", label: "Item rarity", vendor: (s) => (s.mods.rarity = true) },
  { id: "physDamage", tab: "mods", label: "Physical damage", vendor: (s) => (s.mods.physicalDamage = true) },
  { id: "spellDamage", tab: "mods", label: "Spell damage", vendor: (s) => (s.mods.spellDamage = true) },
  { id: "attackSpeed", tab: "mods", label: "Attack speed", vendor: (s) => (s.mods.attackSpeed = true) },
  { id: "castSpeed", tab: "mods", label: "Cast speed", vendor: (s) => (s.mods.castSpeed = true) },
  { id: "allSkills", tab: "mods", label: "+ all skills", vendor: (s) => (s.mods.skillsAll = true) },

  { id: "waystoneClass", tab: "maps", label: "Waystones", detail: "Item class/name", term: DIRECT("waystone|map") },
  { id: "mapRare", tab: "maps", label: "Rare maps", vendor: (s) => (s.rarity.rare = true) },
  { id: "mapQuant", tab: "maps", label: "Quantity", term: DIRECT("quant") },
  { id: "mapPack", tab: "maps", label: "Pack size", term: DIRECT("pack") },
  { id: "mapRarity", tab: "maps", label: "Map rarity", term: DIRECT("rarity") },

  { id: "tabletClass", tab: "tablet", label: "Precursor tablets", term: DIRECT("tablet|tower") },
  { id: "tabletRare", tab: "tablet", label: "Rare tablets", vendor: (s) => (s.rarity.rare = true) },
  { id: "tabletQuant", tab: "tablet", label: "Quantity", term: DIRECT("quant") },
  { id: "tabletPack", tab: "tablet", label: "Pack size", term: DIRECT("pack") },
];

function poolTab(pool: keyof RegexOverlayData): RegexControl["tab"] {
  if (pool === "waystoneMods") return "maps";
  if (pool === "tabletMods") return "tablet";
  return "mods";
}

function poolPrefix(pool: keyof RegexOverlayData): string {
  if (pool === "waystoneMods") return "map";
  if (pool === "tabletMods") return "tablet";
  return "mod";
}

function rankedPool(pool: EligibleModView[] | null | undefined): EligibleModView[] {
  return [...(pool ?? [])]
    .filter((m) => viewLines(m).length > 0)
    .sort((a, b) => {
      if (a.eligible_now !== b.eligible_now) return a.eligible_now ? -1 : 1;
      if (b.weight !== a.weight) return b.weight - a.weight;
      if (a.tier_index !== b.tier_index) return a.tier_index - b.tier_index;
      return (a.name ?? a.mod_id).localeCompare(b.name ?? b.mod_id);
    });
}

function poolControls(data: RegexOverlayData | undefined, pool: keyof RegexOverlayData): RegexControl[] {
  const mods = rankedPool(data?.[pool]).slice(0, POOL_LIMITS[pool]);
  return mods.map((m) => {
    const text = modText(m.text_template);
    return {
      id: `${poolPrefix(pool)}:${m.mod_id}`,
      tab: poolTab(pool),
      label: m.name ?? humanizeModId(m.mod_id),
      detail: text ? `T${m.tier_index} ${text}` : `T${m.tier_index}`,
      pool,
      modId: m.mod_id,
    };
  });
}

function controlsForData(data?: RegexOverlayData): RegexControl[] {
  return [
    ...STATIC_CONTROLS,
    ...poolControls(data, "itemMods"),
    ...poolControls(data, "waystoneMods"),
    ...poolControls(data, "tabletMods"),
  ];
}

function normalizedState(state: RegexOverlayState, data?: RegexOverlayData): RegexOverlayState {
  const controls = controlsForData(data);
  const activeTab = TABS.some((t) => t.id === state.activeTab) ? state.activeTab : TABS[0].id;
  const selected = state.selected.filter(
    (id, i, arr) => controls.some((c) => c.id === id && !c.disabled) && arr.indexOf(id) === i,
  );
  const visible = controls.map((c, i) => (c.tab === activeTab ? i : -1)).filter((i) => i >= 0);
  const clamped = Math.max(0, Math.min(state.focusIndex, controls.length - 1));
  const focusIndex = visible.includes(clamped) ? clamped : (visible[0] ?? 0);
  return { activeTab, focusIndex, selected };
}

function visibleIndices(state: RegexOverlayState, data?: RegexOverlayData): number[] {
  const controls = controlsForData(data);
  return controls.map((c, i) => (c.tab === state.activeTab ? i : -1)).filter((i) => i >= 0);
}

function findPoolMod(
  control: RegexControl,
  data: RegexOverlayData | undefined,
): { view: EligibleModView; pool: EligibleModView[] } | null {
  if (!control.pool || !control.modId) return null;
  const pool = data?.[control.pool] ?? [];
  const view = pool.find((m) => m.mod_id === control.modId);
  return view ? { view, pool } : null;
}

function termForControl(control: RegexControl, data?: RegexOverlayData): SearchTerm | null {
  if (control.term) return control.term;
  const poolMod = findPoolMod(control, data);
  if (!poolMod) return null;
  const result = termsForMods([poolMod.view], poolMod.pool);
  if (result.terms.length === 0) return null;
  return { pattern: result.terms.map((t) => t.pattern).join("|") };
}

export function emptyRegexOverlayState(): RegexOverlayState {
  return { activeTab: TABS[0].id, focusIndex: 0, selected: [] };
}

export function moveRegexFocus(
  state: RegexOverlayState,
  delta: number,
  data?: RegexOverlayData,
): RegexOverlayState {
  const s = normalizedState(state, data);
  const visible = visibleIndices(s, data);
  if (visible.length === 0 || delta === 0) return s;
  const current = Math.max(0, visible.indexOf(s.focusIndex));
  const next = visible[(current + delta + visible.length) % visible.length] ?? visible[0];
  return { ...s, focusIndex: next };
}

export function moveRegexTab(
  state: RegexOverlayState,
  delta: number,
  data?: RegexOverlayData,
): RegexOverlayState {
  const s = normalizedState(state, data);
  const current = Math.max(0, TABS.findIndex((t) => t.id === s.activeTab));
  const nextTab = TABS[(current + delta + TABS.length) % TABS.length];
  const controls = controlsForData(data);
  const nextFocus = controls.findIndex((c) => c.tab === nextTab.id);
  return { ...s, activeTab: nextTab.id, focusIndex: Math.max(0, nextFocus) };
}

export function toggleRegexFocused(
  state: RegexOverlayState,
  data?: RegexOverlayData,
): RegexOverlayState {
  const s = normalizedState(state, data);
  const control = controlsForData(data)[s.focusIndex];
  if (!control || control.disabled) return s;
  const selected = s.selected.includes(control.id)
    ? s.selected.filter((id) => id !== control.id)
    : [...s.selected, control.id];
  return { ...s, selected };
}

export function regexForState(
  state: RegexOverlayState,
  data?: RegexOverlayData,
): ReturnType<typeof assembleSearch> {
  const s = normalizedState(state, data);
  const controls = controlsForData(data);
  const vendor = emptyVendorSettings();
  const direct: SearchTerm[] = [];
  for (const id of s.selected) {
    const control = controls.find((c) => c.id === id);
    if (!control) continue;
    control.vendor?.(vendor);
    const term = termForControl(control, data);
    if (term) direct.push(term);
  }
  vendor.classes = vendor.classes.filter((klass, i, arr) => VENDOR_CLASSES.includes(klass) && arr.indexOf(klass) === i);
  return assembleSearch([...vendorTerms(vendor), ...direct], "all");
}

export type RegexClipboardResult =
  | {
      ok: true;
      text: string;
      label: string;
      length: number;
      detail: string;
    }
  | {
      ok: false;
      reason: string;
    };

export function regexClipboardResult(
  state: RegexOverlayState,
  data: RegexOverlayData | undefined,
  apply: boolean,
): RegexClipboardResult {
  const assembled = regexForState(state, data);
  if (assembled.value === "") return { ok: false, reason: "select at least one filter" };
  if (assembled.overBudget) return { ok: false, reason: "search string over budget" };
  return {
    ok: true,
    text: assembled.value,
    label: apply ? "copied for paste" : "copied",
    length: assembled.length,
    detail: assembled.value,
  };
}

export function regexMenuPayload(
  state: RegexOverlayState,
  rect: { x: number; y: number; width: number; height: number },
  data?: RegexOverlayData,
): HyprOverlayPayload {
  const s = normalizedState(state, data);
  const controls = controlsForData(data);
  const assembled = regexForState(s, data);
  return {
    mode: "menu",
    visible: true,
    rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
    menu: {
      title: "Search Regex",
      subtitle: "Items, mods, maps, tablets",
      activeTab: s.activeTab,
      focusIndex: s.focusIndex,
      tabs: [...TABS],
      preview: assembled.value || "select filters",
      budget: `${assembled.length} / ${MAX_SEARCH_LENGTH}`,
      footer: "Up/Down move - Left/Right tab - Enter toggle - Copy writes clipboard",
      controls: controls.map((control) => ({
        id: control.id,
        tab: control.tab,
        label: control.label,
        value: termForControl(control, data)?.pattern,
        detail: control.detail,
        kind: "toggle" as const,
        selected: s.selected.includes(control.id),
        disabled: control.disabled,
      })),
    },
  };
}
