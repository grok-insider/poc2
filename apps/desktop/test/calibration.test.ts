import { describe, expect, test } from "bun:test";
import { parseSlurpRect, selectSlurpRegion } from "../src/capture/calibration";

describe("slurp calibration", () => {
  test("parses positive and negative global coordinates", () => {
    expect(parseSlurpRect("405,170 200x160\n")).toEqual({
      x: 405,
      y: 170,
      width: 200,
      height: 160,
    });
    expect(parseSlurpRect("-1920,40 640x300")).toEqual({
      x: -1920,
      y: 40,
      width: 640,
      height: 300,
    });
  });

  test("rejects malformed and empty rectangles", () => {
    expect(parseSlurpRect("cancelled")).toBeNull();
    expect(parseSlurpRect("0,0 0x20")).toBeNull();
  });

  test("invokes slurp with a visible selection style", async () => {
    const calls: Array<{ command: string; args: string[] }> = [];
    const rect = await selectSlurpRegion(async (command, args) => {
      calls.push({ command, args });
      return { stdout: "10,20 300x90\n", stderr: "" };
    });
    expect(rect).toEqual({ x: 10, y: 20, width: 300, height: 90 });
    expect(calls[0]?.command).toBe("slurp");
    expect(calls[0]?.args).toContain("%x,%y %wx%h");
  });
});
