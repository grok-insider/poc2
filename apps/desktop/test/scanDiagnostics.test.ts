import { describe, expect, test } from "bun:test";
import { getScanDiagnostics, setScanDiagnostics } from "../src/scanDiagnostics";

describe("scan diagnostics", () => {
  test("stores bounded renderer diagnostics", () => {
    const stored = setScanDiagnostics({
      updatedAt: "2026-07-10T00:00:00.000Z",
      transport: "hyprland-plugin",
      captureWidth: 320,
      selectedScale: 2,
      fastOcrMs: 1250,
      totalMs: 1500,
      rawText: "1x Uhtred’s Saga\n3x Greater Chaos Orb",
      resolvedRows: ["Uhtred's Saga", "3x Greater Chaos Orb"],
      lineRows: ["20% 1.2 div Uhtred's Saga"],
      pluginProtocol: 4,
      renderOk: true,
      watcherEnabled: false,
    });
    expect(stored?.transport).toBe("hyprland-plugin");
    expect(getScanDiagnostics()?.captureWidth).toBe(320);
    expect(getScanDiagnostics()?.pluginProtocol).toBe(4);
    expect(getScanDiagnostics()?.totalMs).toBe(1500);
  });

  test("rejects malformed values", () => {
    expect(setScanDiagnostics({ transport: "unknown" })).toBeNull();
  });
});
