import { describe, expect, test } from "bun:test";
import {
  type CaptureRect,
  type DisplayBounds,
  coerceRect,
  cropForDisplay,
  intersectionArea,
  isValidRect,
  pickDisplay,
} from "../src/capture/screen";

const rect = (x: number, y: number, width: number, height: number): CaptureRect => ({
  x,
  y,
  width,
  height,
});

describe("isValidRect", () => {
  test("accepts a normal rect", () => {
    expect(isValidRect(rect(10, 20, 100, 40))).toBe(true);
  });
  test("rejects zero/negative size", () => {
    expect(isValidRect(rect(0, 0, 0, 10))).toBe(false);
    expect(isValidRect(rect(0, 0, 10, 0))).toBe(false);
    expect(isValidRect(rect(0, 0, -5, 10))).toBe(false);
  });
  test("rejects non-finite", () => {
    expect(isValidRect(rect(NaN, 0, 10, 10))).toBe(false);
    expect(isValidRect(rect(0, 0, Infinity, 10))).toBe(false);
  });
});

describe("coerceRect", () => {
  test("parses string numbers from untrusted IPC", () => {
    expect(coerceRect({ x: "10", y: "20", width: "30", height: "40" })).toEqual(
      rect(10, 20, 30, 40),
    );
  });
  test("rejects non-objects and malformed payloads", () => {
    expect(coerceRect(null)).toBeNull();
    expect(coerceRect("nope")).toBeNull();
    expect(coerceRect({ x: 1, y: 2 })).toBeNull(); // width/height → NaN
    expect(coerceRect({ x: 0, y: 0, width: 0, height: 0 })).toBeNull();
  });
});

describe("intersectionArea", () => {
  const display = rect(0, 0, 1920, 1080);
  test("fully inside → rect area", () => {
    expect(intersectionArea(rect(100, 100, 200, 50), display)).toBe(200 * 50);
  });
  test("disjoint → 0", () => {
    expect(intersectionArea(rect(2000, 100, 100, 100), display)).toBe(0);
  });
  test("partial overlap → clipped area", () => {
    // Rect straddles the right edge by 100px.
    expect(intersectionArea(rect(1820, 0, 200, 100), display)).toBe(100 * 100);
  });
});

describe("pickDisplay", () => {
  const displays: DisplayBounds[] = [
    { id: 1, bounds: rect(0, 0, 1920, 1080), scaleFactor: 1 },
    { id: 2, bounds: rect(1920, 0, 2560, 1440), scaleFactor: 2 },
  ];
  test("picks the display containing the rect", () => {
    expect(pickDisplay(rect(2000, 100, 200, 80), displays)?.id).toBe(2);
  });
  test("picks the display with the larger overlap when straddling", () => {
    // 1900..2100: 20px on display 1, 180px on display 2 → display 2 wins.
    expect(pickDisplay(rect(1900, 100, 200, 80), displays)?.id).toBe(2);
  });
  test("returns null when the rect touches no display", () => {
    expect(pickDisplay(rect(-500, -500, 100, 100), displays)).toBeNull();
  });
});

describe("cropForDisplay", () => {
  test("maps global coords to display-local source pixels at scale 1", () => {
    const display: DisplayBounds = { id: 2, bounds: rect(1920, 0, 1920, 1080), scaleFactor: 1 };
    expect(cropForDisplay(rect(2020, 100, 200, 50), display)).toEqual(
      rect(100, 100, 200, 50),
    );
  });
  test("applies the scale factor for HiDPI displays", () => {
    const display: DisplayBounds = { id: 2, bounds: rect(0, 0, 2560, 1440), scaleFactor: 2 };
    expect(cropForDisplay(rect(100, 50, 200, 80), display)).toEqual(
      rect(200, 100, 400, 160),
    );
  });
  test("clamps a rect that overruns the display edge", () => {
    const display: DisplayBounds = { id: 1, bounds: rect(0, 0, 1920, 1080), scaleFactor: 1 };
    const crop = cropForDisplay(rect(1820, 1000, 400, 400), display);
    // local origin 1820,1000 → max 100 wide, 80 tall.
    expect(crop).toEqual(rect(1820, 1000, 100, 80));
  });
});
