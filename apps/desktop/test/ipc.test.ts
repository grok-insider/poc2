import { describe, expect, test } from "bun:test";
import { isAllowlistedUrl } from "../src/fetchAllowlist";
import { coerceRect } from "../src/capture/screen";

// `registerIpc` needs an Electron runtime, so we don't exercise the handlers
// here; instead we cover the pure argument-validation the handlers delegate to
// (the same functions the IPC layer calls before touching any window/net API).
// `isAllowlistedUrl`/`FETCH_ALLOWLIST` live in the electron-free fetchAllowlist
// module (ipc.ts re-exports them) precisely so this stays bun-testable.

describe("isAllowlistedUrl (FETCH_ALLOWLIST)", () => {
  test("allows poe2scout over https", () => {
    expect(isAllowlistedUrl("https://poe2scout.com/api/items")).toBe(true);
  });
  test("allows www.pathofexile.com over https", () => {
    expect(isAllowlistedUrl("https://www.pathofexile.com/api/trade2")).toBe(true);
  });
  test("allows poe.ninja over https (this worker's line)", () => {
    expect(isAllowlistedUrl("https://poe.ninja/api/data")).toBe(true);
  });
  test("rejects http (downgrade) and unknown hosts", () => {
    expect(isAllowlistedUrl("http://poe.ninja/api")).toBe(false);
    expect(isAllowlistedUrl("https://evil.example.com/poe.ninja")).toBe(false);
  });
  test("rejects garbage", () => {
    expect(isAllowlistedUrl("not a url")).toBe(false);
  });
});

describe("capture-region IPC arg validation (coerceRect)", () => {
  test("a valid rect survives the boundary", () => {
    expect(coerceRect({ x: 10, y: 20, width: 200, height: 60 })).toEqual({
      x: 10,
      y: 20,
      width: 200,
      height: 60,
    });
  });
  test("malformed payloads are rejected before capture runs", () => {
    expect(coerceRect(undefined)).toBeNull();
    expect(coerceRect(null)).toBeNull();
    expect(coerceRect({ x: 0, y: 0 })).toBeNull();
    expect(coerceRect({ x: 0, y: 0, width: -1, height: 5 })).toBeNull();
  });
});
