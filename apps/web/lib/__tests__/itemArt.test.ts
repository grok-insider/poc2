import { describe, expect, test } from "bun:test";
import { baseIconUrlByName } from "../baseIcons";
import {
  itemArtSlug,
  resolveItemArt,
  uniqueIconUrl,
  type UniqueIconManifest,
} from "../itemArt";
import type { BaseIconManifest } from "../types";

const baseManifest: BaseIconManifest = {
  version: 2,
  fetched_at: "test",
  entries: {
    "Metadata/Items/Armours/Gloves/FourGlovesStr1": {
      name: "Stocky Mitts",
      class_pascal: "Gloves",
      rel: "Gloves/GlovesStr01.webp",
      source_url: "https://example.test/Stocky_Mitts",
      drop_level: 1,
      attribute_pool: "Str",
    },
    "Metadata/Items/Weapons/TwoHandWeapons/Bows/FourBow1": {
      name: "Crude Bow",
      class_pascal: "Bow",
      rel: "Bow/Bow01.webp",
      source_url: "https://example.test/Crude_Bow",
      drop_level: 1,
      attribute_pool: "Dex",
    },
  },
  missing: [],
};

const uniqueManifest: UniqueIconManifest = {
  version: 1,
  fetched_at: "test",
  entries: {
    facebreaker: {
      name: "Facebreaker",
      rel: "Facebreaker.webp",
      source_url: "https://cdn.poe2db.tw/image/Art/2DItems/Armours/Gloves/Uniques/Facebreaker.webp",
    },
  },
};

describe("itemArt", () => {
  test("baseIconUrlByName is case-insensitive", () => {
    expect(baseIconUrlByName(baseManifest, "stocky mitts")).toBe(
      "/base-icons/Gloves/GlovesStr01.webp",
    );
    expect(baseIconUrlByName(baseManifest, "Crude Bow")).toBe("/base-icons/Bow/Bow01.webp");
    expect(baseIconUrlByName(baseManifest, "missing")).toBeNull();
  });

  test("unique art preferred for unique kind", () => {
    const art = resolveItemArt({
      kind: "unique",
      name: "Facebreaker",
      typeLine: "Stocky Mitts",
      baseId: "Metadata/Items/Armours/Gloves/FourGlovesStr1",
      baseManifest,
      uniqueManifest,
    });
    expect(art).toEqual({
      url: "/unique-icons/Facebreaker.webp",
      source: "unique-local",
    });
  });

  test("falls back to base art by typeLine", () => {
    const art = resolveItemArt({
      kind: "rare",
      name: "Corruption Carapace",
      typeLine: "Stocky Mitts",
      baseManifest,
      uniqueManifest,
    });
    expect(art?.url).toBe("/base-icons/Gloves/GlovesStr01.webp");
    expect(art?.source).toBe("base");
  });

  test("itemArtSlug strips punctuation", () => {
    expect(itemArtSlug("Facebreaker")).toBe("Facebreaker");
    expect(itemArtSlug("Brutus's Lead Sprinkler")).toMatch(/Brutus/i);
  });

  test("uniqueIconUrl looks up by lowercased name", () => {
    expect(uniqueIconUrl(uniqueManifest, "FACEBREAKER")).toBe(
      "/unique-icons/Facebreaker.webp",
    );
  });

  test("missing manifests soft-fail to null art (no throw)", () => {
    expect(
      resolveItemArt({
        kind: "unique",
        name: "Facebreaker",
        typeLine: "Stocky Mitts",
        baseManifest: null,
        uniqueManifest: null,
      }),
    ).toBeNull();
    expect(
      resolveItemArt({
        kind: "rare",
        name: "Whatever",
        typeLine: "Unknown Base",
        baseManifest: null,
        uniqueManifest: null,
      }),
    ).toBeNull();
    expect(uniqueIconUrl(null, "Facebreaker")).toBeNull();
    expect(baseIconUrlByName(null, "Stocky Mitts")).toBeNull();
  });
});
