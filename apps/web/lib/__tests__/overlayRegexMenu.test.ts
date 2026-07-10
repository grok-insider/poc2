import { describe, expect, test } from "bun:test";
import {
  emptyRegexOverlayState,
  moveRegexFocus,
  moveRegexTab,
  regexClipboardResult,
  type RegexOverlayData,
  regexForState,
  regexMenuPayload,
  toggleRegexFocused,
} from "../overlay/regexMenu";
import type { EligibleModView } from "../types";

function view(partial: Partial<EligibleModView> & { mod_id: string }): EligibleModView {
  return {
    name: null,
    mod_group: partial.mod_id.replace(/\d+$/, ""),
    affix_type: "prefix",
    kind: "explicit",
    concepts: [],
    tags: [],
    tier_index: 1,
    tier_count: 1,
    required_level: 1,
    eligible_now: true,
    blocked_by_min_level: false,
    blocked_by_group: false,
    weight: 100,
    weight_share: 0.1,
    text_template: null,
    stats: [],
    is_hybrid: false,
    is_essence_only: false,
    is_desecrated_only: false,
    is_local: false,
    ...partial,
  };
}

const DATA: RegexOverlayData = {
  itemMods: [
    view({
      mod_id: "IncreasedMana1",
      name: "Fecundity",
      concepts: ["Mana"],
      text_template: "+(40-59) to maximum Mana",
      stats: [{ stat_id: "mana", min: 40, max: 59 }],
    }),
  ],
  waystoneMods: [
    view({
      mod_id: "MapPackSize1",
      name: "Teeming",
      concepts: ["PackSize"],
      text_template: "(12-18)% increased Pack Size",
      stats: [{ stat_id: "pack_size", min: 12, max: 18 }],
    }),
  ],
};

describe("overlay regex menu", () => {
  test("focus navigation and toggle updates the generated regex", () => {
    let state = emptyRegexOverlayState();
    state = toggleRegexFocused(state); // Rare
    const rare = regexForState(state);
    expect(rare.value).toContain("y: r");

    state = moveRegexFocus(state, 1);
    state = toggleRegexFocused(state); // Magic
    const both = regexForState(state);
    expect(both.value).toContain("y: (r|m)");
  });

  test("tab navigation moves focus to that tab's first control", () => {
    let state = emptyRegexOverlayState();
    state = moveRegexTab(state, 1);
    expect(state.activeTab).toBe("props");
    const payload = regexMenuPayload(state, { x: 1, y: 2, width: 500, height: 300 });
    expect(payload.mode).toBe("menu");
    expect(payload.menu?.activeTab).toBe("props");
    expect(payload.menu?.controls?.some((c) => c.tab === "maps")).toBe(true);
  });

  test("direct map/tablet terms stay clean-room and bounded", () => {
    let state = emptyRegexOverlayState();
    state = moveRegexTab(state, 3); // maps
    state = toggleRegexFocused(state); // waystone/map
    const assembled = regexForState(state);
    expect(assembled.value).toContain("waystone|map");
    expect(assembled.overBudget).toBe(false);
  });

  test("eligible mod pools add data-backed menu controls", () => {
    const payload = regexMenuPayload(emptyRegexOverlayState(), { x: 1, y: 2, width: 500, height: 300 }, DATA);
    expect(payload.menu?.controls?.length ?? 0).toBeLessThanOrEqual(64);
    const modIndex = payload.menu?.controls?.findIndex((c) => c.id === "mod:IncreasedMana1") ?? -1;
    expect(modIndex).toBeGreaterThan(0);

    const selected = toggleRegexFocused(
      { activeTab: "mods", focusIndex: modIndex, selected: [] },
      DATA,
    );
    const assembled = regexForState(selected, DATA);
    const pattern = payload.menu?.controls?.[modIndex]?.value;
    expect(pattern).toBeTruthy();
    expect(assembled.value).toContain(pattern as string);
    expect(new RegExp(pattern as string).test("+52 to maximum mana")).toBe(true);
  });

  test("waystone pool controls use generated mod regex terms", () => {
    const payload = regexMenuPayload(
      { activeTab: "maps", focusIndex: 0, selected: [] },
      { x: 1, y: 2, width: 500, height: 300 },
      DATA,
    );
    const mapIndex = payload.menu?.controls?.findIndex((c) => c.id === "map:MapPackSize1") ?? -1;
    expect(mapIndex).toBeGreaterThan(0);

    const selected = toggleRegexFocused(
      { activeTab: "maps", focusIndex: mapIndex, selected: [] },
      DATA,
    );
    const assembled = regexForState(selected, DATA);
    const pattern = payload.menu?.controls?.[mapIndex]?.value;
    expect(pattern).toBeTruthy();
    expect(assembled.value).toContain(pattern as string);
    expect(new RegExp(pattern as string).test("15% increased pack size")).toBe(true);
  });

  test("clipboard/apply result validates empty state and labels apply copies", () => {
    expect(regexClipboardResult(emptyRegexOverlayState(), undefined, false)).toEqual({
      ok: false,
      reason: "select at least one filter",
    });

    const selected = toggleRegexFocused(emptyRegexOverlayState());
    const copied = regexClipboardResult(selected, undefined, true);
    expect(copied).toMatchObject({
      ok: true,
      label: "copied for paste",
    });
    if (copied.ok) {
      expect(copied.text).toContain("y: r");
      expect(copied.length).toBe(copied.text.length);
    }
  });
});
