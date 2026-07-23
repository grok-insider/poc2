import { describe, expect, test } from "bun:test";
import {
  applyUpdateEvent,
  clampPercent,
  initialUpdateStatus,
  updateStatusLabel,
} from "../src/updaterState";

const NOW = () => "2026-07-18T12:00:00.000Z";

describe("initialUpdateStatus", () => {
  test("enabled idle snapshot", () => {
    expect(initialUpdateStatus("1.2.3", true)).toEqual({
      enabled: true,
      phase: "idle",
      currentVersion: "1.2.3",
      availableVersion: null,
      percent: null,
      error: null,
      checkedAt: null,
    });
  });

  test("disabled snapshot (dev / unpackaged)", () => {
    const s = initialUpdateStatus("1.2.3", false);
    expect(s.enabled).toBe(false);
    expect(s.phase).toBe("idle");
  });
});

describe("clampPercent", () => {
  test("clamps and rounds", () => {
    expect(clampPercent(-5)).toBe(0);
    expect(clampPercent(150)).toBe(100);
    expect(clampPercent(33.333)).toBe(33.3);
    expect(clampPercent(Number.NaN)).toBe(0);
  });
});

describe("applyUpdateEvent", () => {
  test("disabled status ignores events", () => {
    const base = initialUpdateStatus("1.0.0", false);
    expect(applyUpdateEvent(base, { type: "checking" }, NOW)).toBe(base);
    expect(
      applyUpdateEvent(base, { type: "available", version: "2.0.0" }, NOW),
    ).toBe(base);
  });

  test("checking → available → progress → downloaded", () => {
    let s = initialUpdateStatus("1.0.0", true);
    s = applyUpdateEvent(s, { type: "checking" }, NOW);
    expect(s.phase).toBe("checking");
    expect(s.error).toBeNull();

    s = applyUpdateEvent(s, { type: "available", version: "1.1.0" }, NOW);
    expect(s.phase).toBe("available");
    expect(s.availableVersion).toBe("1.1.0");
    expect(s.checkedAt).toBe("2026-07-18T12:00:00.000Z");

    s = applyUpdateEvent(s, { type: "progress", percent: 42.5 }, NOW);
    expect(s.phase).toBe("downloading");
    expect(s.percent).toBe(42.5);

    s = applyUpdateEvent(s, { type: "downloaded", version: "1.1.0" }, NOW);
    expect(s.phase).toBe("downloaded");
    expect(s.percent).toBe(100);
    expect(s.availableVersion).toBe("1.1.0");
  });

  test("not-available clears available version", () => {
    let s = initialUpdateStatus("1.0.0", true);
    s = applyUpdateEvent(s, { type: "available", version: "1.1.0" }, NOW);
    s = applyUpdateEvent(s, { type: "not-available" }, NOW);
    expect(s.phase).toBe("not-available");
    expect(s.availableVersion).toBeNull();
    expect(s.checkedAt).toBe("2026-07-18T12:00:00.000Z");
  });

  test("error records message", () => {
    let s = initialUpdateStatus("1.0.0", true);
    s = applyUpdateEvent(s, { type: "error", message: "network down" }, NOW);
    expect(s.phase).toBe("error");
    expect(s.error).toBe("network down");
    expect(s.checkedAt).toBe("2026-07-18T12:00:00.000Z");
  });
});

describe("updateStatusLabel", () => {
  test("covers main phases", () => {
    const base = initialUpdateStatus("1.0.0", true);
    expect(updateStatusLabel(initialUpdateStatus("1.0.0", false))).toContain(
      "disabled",
    );
    expect(updateStatusLabel(base)).toContain("No update");
    expect(
      updateStatusLabel(
        applyUpdateEvent(base, { type: "downloaded", version: "2.0.0" }, NOW),
      ),
    ).toContain("2.0.0");
    expect(
      updateStatusLabel(
        applyUpdateEvent(base, { type: "error", message: "boom" }, NOW),
      ),
    ).toContain("boom");
  });
});
