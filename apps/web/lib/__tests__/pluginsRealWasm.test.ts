import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import path from "node:path";
import {
  callPredicate,
  extractContent,
  hasPredicateSurface,
  instantiatePlugin,
} from "../plugins/abi";

/// End-to-end host test against the REAL SDK-built example plugin
/// (examples/plugins/predicate-ilvl-min). Skips when the wasm artifact
/// hasn't been built locally:
///   cd examples/plugins/predicate-ilvl-min &&
///   cargo build --release --target wasm32-unknown-unknown
///
/// This is the test that would have caught the v1 SDK arena bug
/// (write_output indexing the arena Vec with absolute addresses).

const WASM = path.join(
  import.meta.dir,
  "../../../../examples/plugins/predicate-ilvl-min/target/wasm32-unknown-unknown/release/predicate_ilvl_min.wasm",
);

describe("real SDK plugin (predicate-ilvl-min)", () => {
  const available = existsSync(WASM);

  test.skipIf(!available)("emits its rule TOML and answers predicates", async () => {
    const bytes = await Bun.file(WASM).arrayBuffer();
    const exports = await instantiatePlugin(bytes);

    // Phase 1: emission.
    const content = extractContent(exports);
    expect(content.rules.length).toBe(1);
    expect(content.rules[0]).toContain("R-PLUGIN-ilvl-min-transmute");
    expect(content.strategies.length).toBe(0);

    // Phase 2: live predicate dispatch, including arena reuse.
    expect(hasPredicateSurface(exports)).toBe(true);
    expect(callPredicate(exports, "ilvl_at_least", '{"ilvl":82}', '{"min":82}')).toBe(true);
    expect(callPredicate(exports, "ilvl_at_least", '{"ilvl":70}', '{"min":82}')).toBe(false);
    expect(callPredicate(exports, "wrong_name", '{"ilvl":99}', '{"min":1}')).toBe(false);
    // Emission after predicate calls (arena reset must not corrupt).
    expect(extractContent(exports).rules.length).toBe(1);
  });

  if (!available) {
    test("artifact not built — skipped (see file header for the build command)", () => {
      expect(true).toBe(true);
    });
  }
});
