import { describe, expect, test } from "bun:test";
import path from "node:path";
import { resolveAppUrl, wantsSpaFallback } from "../src/staticResolve";

const ROOT = path.sep === "/" ? "/srv/web-out" : "C:\\srv\\web-out";
const j = (...parts: string[]) => path.join(ROOT, ...parts);

describe("resolveAppUrl", () => {
  test("serves index for the root URL", () => {
    expect(resolveAppUrl(ROOT, "app://poc2/")).toBe(j("index.html"));
  });

  test("maps root-absolute asset paths into the export", () => {
    expect(resolveAppUrl(ROOT, "app://poc2/wasm/poc2_wasm_bg.wasm")).toBe(
      j("wasm", "poc2_wasm_bg.wasm"),
    );
    expect(resolveAppUrl(ROOT, "app://poc2/_next/static/chunks/main.js")).toBe(
      j("_next", "static", "chunks", "main.js"),
    );
  });

  test("decodes percent-encoding", () => {
    expect(resolveAppUrl(ROOT, "app://poc2/base-icons/Two%20Hand.webp")).toBe(
      j("base-icons", "Two Hand.webp"),
    );
  });

  test("never escapes the export root", () => {
    // Literal and %2e-encoded dot segments are collapsed by the URL parser
    // itself; the explicit guard covers what survives (e.g. encoded
    // backslash traversal on Windows). The invariant: every resolution is
    // null or stays inside root.
    const hostile = [
      "app://poc2/../../etc/passwd",
      "app://poc2/%2e%2e/%2e%2e/etc/passwd",
      "app://poc2/..%5C..%5Cwindows%5Cwin.ini",
      "app://poc2/a/../../../b",
      "app://poc2//etc/passwd",
      "app://poc2/%c0%ae%c0%ae/x",
    ];
    for (const url of hostile) {
      const r = resolveAppUrl(ROOT, url);
      const inside = r === null || r === ROOT || r.startsWith(ROOT + path.sep);
      expect(inside).toBe(true);
    }
  });

  test("rejects unparseable URLs", () => {
    expect(resolveAppUrl(ROOT, "not a url")).toBeNull();
  });
});

describe("wantsSpaFallback", () => {
  test("extensionless deep links fall back to index", () => {
    expect(wantsSpaFallback(j("some-route"))).toBe(true);
  });
  test("missing assets do not", () => {
    expect(wantsSpaFallback(j("wasm", "missing.wasm"))).toBe(false);
  });
});
