#!/usr/bin/env bun
/**
 * Ensure `apps/web/public/poc2.bundle.json.gz` exists before `bun run build`.
 *
 * The data bundle is gitignored (operator/CI artifact). Packaged desktop
 * installs serve the Next static export, so a missing bundle becomes a
 * hard 404 at boot: "ENGINE FAILED TO LOAD / bundle fetch failed: 404".
 *
 * Usage (repo root):
 *   bun scripts/ensure-web-bundle.mjs
 *   bun scripts/ensure-web-bundle.mjs --force   # rebuild even if present
 *
 * Env:
 *   POC2_BUNDLE_PATCH  default 0.5.0
 */
import { existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = resolve(root, "apps/web/public/poc2.bundle.json.gz");
const force = process.argv.includes("--force");
const patch = process.env.POC2_BUNDLE_PATCH ?? "0.5.0";

if (existsSync(out) && !force) {
  const kb = Math.round(statSync(out).size / 1024);
  console.log(`ensure-web-bundle: already present (${kb} KiB) → ${out}`);
  process.exit(0);
}

mkdirSync(dirname(out), { recursive: true });
console.log(`ensure-web-bundle: building patch ${patch} → ${out}`);

const r = spawnSync(
  "cargo",
  [
    "run",
    "--release",
    "-p",
    "poc2-pipeline",
    "--",
    "build",
    "--out",
    out,
    "--patch",
    patch,
  ],
  { cwd: root, stdio: "inherit", shell: process.platform === "win32" },
);

if (r.status !== 0) {
  console.error("ensure-web-bundle: pipeline build failed");
  process.exit(r.status ?? 1);
}

if (!existsSync(out) || statSync(out).size < 1024) {
  console.error("ensure-web-bundle: output missing or too small:", out);
  process.exit(1);
}

console.log(
  `ensure-web-bundle: ok (${Math.round(statSync(out).size / 1024)} KiB)`,
);
