import { describe, expect, test } from "bun:test";
import {
  captureRegionWithGrim,
  grimGeometry,
} from "../src/capture/grim";
import { waylandDisplayFromInstances } from "../src/capture/wayland";

const PNG_HEADER = Buffer.from("89504e470d0a1a0a", "hex");

describe("grim region capture", () => {
  test("finds the Wayland socket for the active Hyprland instance", () => {
    const raw = JSON.stringify([
      { instance: "other", wl_socket: "wayland-0" },
      { instance: "active", wl_socket: "wayland-1" },
    ]);
    expect(waylandDisplayFromInstances(raw, "active")).toBe("wayland-1");
    expect(waylandDisplayFromInstances("invalid", "active")).toBeNull();
  });

  test("formats global geometry including negative monitor coordinates", () => {
    expect(grimGeometry({ x: -1920, y: 40, width: 640, height: 300 })).toBe(
      "-1920,40 640x300",
    );
  });

  test("returns a PNG data URL and exact logical dimensions", async () => {
    const calls: string[][] = [];
    const result = await captureRegionWithGrim(
      { x: 10, y: 20, width: 300, height: 90 },
      async (args) => {
        calls.push(args);
        return Buffer.concat([PNG_HEADER, Buffer.from("fixture")]);
      },
    );
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.width).toBe(300);
      expect(result.height).toBe(90);
      expect(result.dataUrl).toStartWith("data:image/png;base64,");
    }
    expect(calls[0]).toEqual(["-g", "10,20 300x90", "-t", "png", "-"]);
  });

  test("rejects invalid regions and non-PNG output", async () => {
    expect((await captureRegionWithGrim({ width: 0 }, async () => PNG_HEADER)).ok).toBe(false);
    const bad = await captureRegionWithGrim(
      { x: 0, y: 0, width: 10, height: 10 },
      async () => Buffer.from("not png"),
    );
    expect(bad.ok).toBe(false);
  });
});
