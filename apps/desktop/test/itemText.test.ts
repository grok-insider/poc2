import { describe, expect, test } from "bun:test";
import { DEFAULT_TIMINGS, isPoeItemText } from "../src/capture/itemText";

describe("isPoeItemText", () => {
  test("accepts English item copies", () => {
    expect(isPoeItemText("Item Class: Shields\nRarity: Normal\nEffigial Tower Shield")).toBe(true);
  });

  test("accepts localized clients", () => {
    expect(isPoeItemText("Классификация: Щиты\nРедкость: Обычный")).toBe(true);
    expect(isPoeItemText("物品类别: 盾\n稀有度: 普通")).toBe(true);
  });

  test("rejects arbitrary clipboard content", () => {
    expect(isPoeItemText("")).toBe(false);
    expect(isPoeItemText("hello world")).toBe(false);
    expect(isPoeItemText("Rarity: Rare\nwithout the class line")).toBe(false);
  });
});

test("APT-derived default timings", () => {
  expect(DEFAULT_TIMINGS.pollMs).toBe(48);
  expect(DEFAULT_TIMINGS.timeoutMs).toBe(500);
  expect(DEFAULT_TIMINGS.restoreAfterMs).toBe(120);
});
