import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { resolveWindowsOcrHelperPath } from "../src/ocr/windowsNative";

const tempDirs: string[] = [];

afterEach(() => {
  for (const dir of tempDirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe("Windows OCR helper discovery", () => {
  test("is disabled outside Windows", () => {
    expect(resolveWindowsOcrHelperPath({
      platform: "linux",
      resourcesPath: "/unused",
      appPath: "/unused",
    })).toBeNull();
  });

  test("prefers an explicit existing helper", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "poc2-windows-ocr-"));
    tempDirs.push(dir);
    const helper = path.join(dir, "custom-helper.exe");
    writeFileSync(helper, "fixture");

    expect(resolveWindowsOcrHelperPath({
      platform: "win32",
      resourcesPath: path.join(dir, "resources"),
      appPath: path.join(dir, "app"),
      override: helper,
    })).toBe(helper);
  });
});
