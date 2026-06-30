"use client";

/// Global craft state (Zustand). Single source of truth for the whole Forge
/// console: the item, the goal, the engine's recommendations, the crafting
/// history (with undo), and which rail section is active. Re-plans (debounced)
/// whenever the item, goal, or risk/depth change — the advisor's "re-plan on
/// every state change" behaviour. Persists to IndexedDB.

import { create } from "zustand";
import { FRESH_BODY_ARMOUR, WORKED_EXAMPLE_GOAL } from "./fixtures";
import { loadCraft, saveCraft, type PersistedCraft } from "./persist";
import { seedSpecsFromItem } from "./concepts";
import { validateArchetype, type Archetype } from "./archetypes";
import { loadBaseIconManifest } from "./baseIcons";
import type {
  AdvisorAction,
  BaseIconManifest,
  EligibleModsResponse,
  Goal,
  HistoryEntry,
  Item,
  MaterialUse,
  Recommendation,
  RecordOutcome,
} from "./types";

/** Parse metadata surfaced after a clipboard import (base/class resolution). */
export interface ParseMeta {
  baseResolved: boolean;
  itemClassId: string | null;
  baseDisplayName: string | null;
  warnings: string[];
}

export type Section =
  | "item"
  | "target"
  | "guide"
  | "eligible"
  | "history"
  | "database"
  | "price"
  | "genesis"
  | "tools"
  | "settings";

export interface OutcomeMeta {
  action?: AdvisorAction | null;
  actionLabel?: string | null;
  costDiv?: number | null;
  materials?: MaterialUse[];
}

interface CraftState {
  // engine
  engineReady: boolean;
  engineError: string | null;
  patch: string;
  modCount: number;

  // craft
  item: Item;
  goal: Goal;
  recommendations: Recommendation[];
  planning: boolean;
  error: string | null;
  risk: number;
  depth: number;

  // workflow
  history: HistoryEntry[]; // newest first
  section: Section;
  outcomeOpen: boolean;

  // base mod pool (for the current item's base + ilvl) — drives the target
  // palette + seed. Computed for a BARE item so the full pool shows regardless
  // of currently-occupied slots.
  eligible: EligibleModsResponse | null;
  eligibleLoading: boolean;
  lastParse: ParseMeta | null;
  /** Where the last imported item text came from (desktop capture vs. paste). */
  lastImportSource: "capture" | "paste" | null;
  /** Raw clipboard text of the last imported item — the price check matches
   * its stat lines against trade ids. Persisted. */
  lastItemText: string | null;
  /** Lines of the last import that resolved to no modifier — shown in the
   * Item pane for both pasted and captured imports. */
  lastUnresolved: string[];

  // base-item icons (scraped, optional)
  iconManifest: BaseIconManifest | null;

  // settings / misc
  league: string;
  /** Engine League ruleset — "challenge" (Runes of Aldur) or "standard".
   * Gates Recombinator + Corruption/Homogenising omens in 0.5. */
  engineLeague: "standard" | "challenge";
  notes: string;
  hydrated: boolean;

  // capture daemon bridge (ADR-0011; best-effort, desktop-side optional)
  captureStatus: "connected" | "disconnected";
  captureDaemonVersion: string | null;
  captureLastError: string | null;
  /** ISO timestamp of the last successful hotkey capture. */
  captureLastAt: string | null;

  // mutations
  setItem: (i: Item) => void;
  setGoal: (g: Goal) => void;
  setRisk: (r: number) => void;
  setDepth: (d: number) => void;
  setSection: (s: Section) => void;
  setNotes: (n: string) => void;
  setLeague: (l: string) => void;
  setEngineLeague: (l: "standard" | "challenge") => Promise<void>;
  openOutcome: () => void;
  closeOutcome: () => void;

  loadFixture: () => void;
  importText: (text: string) => Promise<string[]>;
  /** Entry point for item text pushed from outside the renderer (desktop
   * capture bridge): imports it and surfaces the Item pane. */
  ingestExternalItemText: (text: string, source: "capture" | "paste") => Promise<string[]>;
  /** Reset import metadata (parse preview, unresolved lines, raw text). */
  clearImport: () => void;
  applyOutcome: (outcome: RecordOutcome, meta?: OutcomeMeta) => Promise<void>;
  undo: () => void;
  clearHistory: () => void;

  refreshEligible: () => Promise<void>;
  seedTargetFromItem: () => void;
  applyArchetype: (arch: Archetype) => void;

  boot: () => Promise<void>;
  replan: () => Promise<void>;
}

let replanToken = 0;
let eligibleToken = 0;

function genId(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  return `h${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
}

function nowIso(): string {
  return new Date().toISOString();
}

/** The persistable slice of the store (the shape `loadCraft` restores). */
function persistedSlice(s: CraftState): PersistedCraft {
  const { item, goal, risk, depth, history, league, engineLeague, notes, lastItemText } = s;
  return { item, goal, risk, depth, history, league, engineLeague, notes, lastItemText };
}

export const useCraft = create<CraftState>((set, get) => ({
  engineReady: false,
  engineError: null,
  patch: "",
  modCount: 0,

  item: FRESH_BODY_ARMOUR,
  goal: WORKED_EXAMPLE_GOAL,
  recommendations: [],
  planning: false,
  error: null,
  risk: 0.5,
  // Depth 4: a magic→rare build (Aug→Regal→Exalt→Exalt) needs ~4 steps to
  // reach a goal-satisfying state, so the beam must look that far ahead or
  // building paths earn ~0 terminal progress and get pruned in favour of
  // cheap no-ops. (mc_samples stays at the wasm default of 50.)
  depth: 4,

  history: [],
  section: "guide",
  outcomeOpen: false,

  eligible: null,
  eligibleLoading: false,
  lastParse: null,
  lastImportSource: null,
  lastItemText: null,
  lastUnresolved: [],
  iconManifest: null,

  league: "Runes of Aldur",
  engineLeague: "challenge",
  notes: "",
  hydrated: false,

  captureStatus: "disconnected",
  captureDaemonVersion: null,
  captureLastError: null,
  captureLastAt: null,

  setItem: (item) => {
    set({ item });
    void get().replan();
    void get().refreshEligible();
  },
  setGoal: (goal) => {
    set({ goal });
    void get().replan();
  },
  setRisk: (risk) => {
    set({ risk });
    void get().replan();
  },
  setDepth: (depth) => {
    set({ depth });
    void get().replan();
  },
  setSection: (section) => set({ section }),
  setNotes: (notes) => set({ notes }),
  setLeague: (league) => {
    set({ league });
    // Point the desktop price cache (overlay's price source) at the new league.
    // No-op in a plain browser; fire-and-forget so the UI never blocks on it.
    void import("./desktop")
      .then(({ getDesktopBridge }) => getDesktopBridge()?.pricesSetLeague(league))
      .catch(() => {});
  },
  setEngineLeague: async (engineLeague) => {
    set({ engineLeague });
    try {
      const { engine } = await import("./engine/client");
      await engine.setLeague(engineLeague);
      await get().replan();
    } catch (e) {
      set({ error: String(e) });
    }
  },
  openOutcome: () => set({ outcomeOpen: true }),
  closeOutcome: () => set({ outcomeOpen: false }),

  loadFixture: () => {
    set({ item: { ...FRESH_BODY_ARMOUR }, goal: WORKED_EXAMPLE_GOAL, history: [] });
    void get().replan();
  },

  importText: async (text) => {
    const { engine } = await import("./engine/client");
    const res = await engine.parseItemText(text);
    // The engine `item.base` is now a real bundle base id; carry the friendly
    // name + the bundle id for the UI so cards render nicely.
    const item: Item = {
      ...res.item,
      base_display_name: res.base_display_name ?? null,
      base_type_id: res.base_resolved ? res.item.base : null,
    };
    set({
      item,
      history: [],
      lastItemText: text,
      lastUnresolved: res.unresolved ?? [],
      lastParse: {
        baseResolved: res.base_resolved ?? false,
        itemClassId: res.item_class_id ?? null,
        baseDisplayName: res.base_display_name ?? null,
        warnings: res.warnings ?? [],
      },
    });
    await get().refreshEligible();
    void get().replan();
    return res.unresolved ?? [];
  },

  ingestExternalItemText: async (text, source) => {
    // Surface the Item pane first so a background capture is visible even
    // while the parse is in flight; keep the raw text even if parsing fails.
    set({ lastImportSource: source, lastItemText: text, section: "item" });
    return get().importText(text);
  },

  clearImport: () => {
    set({ lastParse: null, lastUnresolved: [], lastImportSource: null, lastItemText: null });
  },

  applyOutcome: async (outcome, meta = {}) => {
    const { engine } = await import("./engine/client");
    const before = get().item;
    const res = await engine.recordOutcome(before, outcome);
    const entry: HistoryEntry = {
      id: genId(),
      timestamp: nowIso(),
      change: res.change,
      explanation: res.explanation,
      action: meta.action ?? null,
      action_label: meta.actionLabel ?? null,
      cost_div: meta.costDiv ?? null,
      materials: meta.materials ?? [],
      before,
    };
    set({ item: res.item, history: [entry, ...get().history], outcomeOpen: false });
    void get().replan();
  },

  undo: () => {
    const [last, ...rest] = get().history;
    if (!last) return;
    set({ item: last.before, history: rest });
    void get().replan();
  },

  clearHistory: () => set({ history: [] }),

  refreshEligible: async () => {
    if (!get().engineReady) return;
    const token = ++eligibleToken;
    const { engine } = await import("./engine/client");
    const item = get().item;
    // Query a BARE item so the palette/seed see the full base pool (current
    // slots don't block their own groups).
    const bare: Item = { ...item, prefixes: [], suffixes: [] };
    set({ eligibleLoading: true });
    try {
      const resp = await engine.eligibleMods(bare, "either", 0);
      if (token !== eligibleToken) return;
      set({ eligible: resp, eligibleLoading: false });
    } catch {
      if (token !== eligibleToken) return;
      set({ eligible: null, eligibleLoading: false });
    }
  },

  seedTargetFromItem: () => {
    const { item, eligible, goal } = get();
    const seeded = seedSpecsFromItem(item, eligible);
    const next: Goal = {
      ...goal,
      target: { ...goal.target, prefixes: seeded.prefixes, suffixes: seeded.suffixes },
    };
    get().setGoal(next);
  },

  applyArchetype: (arch) => {
    const { eligible, goal } = get();
    const { prefixes, suffixes } = validateArchetype(arch, eligible);
    const next: Goal = { ...goal, target: { ...goal.target, prefixes, suffixes } };
    get().setGoal(next);
  },

  boot: async () => {
    try {
      const saved = await loadCraft();
      if (saved) {
        set({
          item: saved.item ?? FRESH_BODY_ARMOUR,
          goal: saved.goal ?? WORKED_EXAMPLE_GOAL,
          risk: saved.risk ?? 0.5,
          depth: saved.depth ?? 4,
          history: saved.history ?? [],
          league: saved.league ?? "Runes of Aldur",
          engineLeague: saved.engineLeague === "standard" ? "standard" : "challenge",
          notes: saved.notes ?? "",
          lastItemText: saved.lastItemText ?? null,
        });
      }
      set({ hydrated: true });

      // Load the base-icon manifest (best-effort; absent → letter fallbacks).
      void loadBaseIconManifest().then((m) => m && set({ iconManifest: m }));

      const { engine } = await import("./engine/client");
      const [patch, modCount] = await Promise.all([engine.patch(), engine.modCount()]);
      // Sync the persisted engine-League ruleset before the first plan.
      await engine.setLeague(get().engineLeague).catch(() => {});
      set({ engineReady: true, patch, modCount, engineError: null });

      // Capture-daemon bridge (ADR-0011): hotkey-captured items pushed from
      // `poc2-capture serve`. Best-effort — silently retries; browser-only
      // users never notice it.
      const { startCaptureBridge, pngBase64ToBlob } = await import("./captureBridge");
      startCaptureBridge({
        onStatus: (status, version) =>
          set({
            captureStatus: status,
            captureDaemonVersion: version ?? get().captureDaemonVersion,
          }),
        onEvent: (ev) => {
          if (ev.type === "item-text") {
            set({ captureLastAt: nowIso(), captureLastError: null });
            void get().ingestExternalItemText(ev.text, "capture");
          } else if (ev.type === "item-image") {
            void (async () => {
              try {
                const { ocrImageToItemText } = await import("./ocr");
                const res = await ocrImageToItemText(
                  pngBase64ToBlob(ev.png_base64),
                  get().iconManifest,
                );
                if (res.text) {
                  set({
                    captureLastAt: nowIso(),
                    captureLastError: res.warnings.join(" · ") || null,
                  });
                  await get().ingestExternalItemText(res.text, "capture");
                } else {
                  set({
                    captureLastError:
                      res.warnings.join(" · ") || "OCR found no item in the screenshot.",
                  });
                }
              } catch (e) {
                set({ captureLastError: String(e) });
              }
            })();
          } else if (ev.type === "capture-error") {
            set({ captureLastError: ev.message });
          }
        },
      });
      await get().refreshEligible();
      await get().replan();
    } catch (e) {
      set({ engineError: String(e), hydrated: true });
    }
  },

  replan: async () => {
    if (!get().engineReady) return;
    const token = ++replanToken;
    const { engine } = await import("./engine/client");
    const { item, goal, risk, depth } = get();
    set({ planning: true, error: null });
    try {
      const recs = await engine.recommend(item, goal, risk, depth, 5);
      if (token !== replanToken) return; // a newer re-plan superseded this one
      set({ recommendations: recs, planning: false });
    } catch (e) {
      if (token !== replanToken) return;
      set({ error: String(e), planning: false });
    }
  },
}));

// Single persister: mutations replace persisted slices wholesale, so a
// reference compare against the previous state detects every change that
// `loadCraft` would restore. Gated on `hydrated` so boot's restore doesn't
// write back what it just read; `saveCraft` is itself debounced and SSR-safe.
useCraft.subscribe((state, prev) => {
  if (!state.hydrated) return;
  const next = persistedSlice(state);
  const before = persistedSlice(prev);
  const keys = Object.keys(next) as (keyof PersistedCraft)[];
  if (keys.every((k) => next[k] === before[k])) return;
  saveCraft(next);
});

/** Total divine spent across recorded history. */
export function totalSpent(history: HistoryEntry[]): number {
  return history.reduce((sum, h) => sum + (h.cost_div ?? 0), 0);
}
