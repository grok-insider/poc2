"use client";

/// UI state for the Regex panel — a tiny separate store so selections
/// survive rail-section switches (the panel unmounts). Deliberately NOT
/// persisted to IndexedDB: search strings are throwaway tooling state.

import { create } from "zustand";
import { emptyVendorSettings, type VendorSettings } from "./vendor";

export type RegexTab = "goal" | "mods" | "vendor";

export interface ModSelection {
  /** Selected mod ids (per-tier ids; selecting a group selects a tier). */
  selected: string[];
  /** mod_id → minimum roll value ("" = none). */
  minValues: Record<string, string>;
  /** Mod ids the item must NOT have (negated group). */
  unwanted: string[];
  mode: "all" | "any";
}

interface RegexState {
  tab: RegexTab;
  customText: string;
  autoCopy: boolean;
  mods: ModSelection;
  vendor: VendorSettings;

  setTab: (t: RegexTab) => void;
  setCustomText: (t: string) => void;
  setAutoCopy: (v: boolean) => void;
  setMods: (m: Partial<ModSelection>) => void;
  setVendor: (v: VendorSettings) => void;
  resetTab: () => void;
}

const emptyMods = (): ModSelection => ({
  selected: [],
  minValues: {},
  unwanted: [],
  mode: "all",
});

export const useRegex = create<RegexState>((set, get) => ({
  tab: "goal",
  customText: "",
  autoCopy: false,
  mods: emptyMods(),
  vendor: emptyVendorSettings(),

  setTab: (tab) => set({ tab }),
  setCustomText: (customText) => set({ customText }),
  setAutoCopy: (autoCopy) => set({ autoCopy }),
  setMods: (m) => set({ mods: { ...get().mods, ...m } }),
  setVendor: (vendor) => set({ vendor }),
  resetTab: () => {
    const { tab } = get();
    if (tab === "mods") set({ mods: emptyMods(), customText: "" });
    else if (tab === "vendor") set({ vendor: emptyVendorSettings(), customText: "" });
    else set({ customText: "" });
  },
}));
