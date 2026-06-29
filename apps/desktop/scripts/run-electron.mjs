// Launch Electron portably (NixOS / Linux / Windows).
//
// The npm package's downloaded binary is a generic FHS build that cannot
// run on NixOS (missing system .so's), while CI/Windows have no `electron`
// on PATH. Resolution order:
//   1. $POC2_ELECTRON (explicit override)
//   2. Linux: `electron` on PATH (nixpkgs devshell) — FHS-safe choice
//   3. the npm package's downloaded binary, if present
//   4. `electron` on PATH (last resort elsewhere)
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const appDir = path.join(here, "..");

function npmElectron() {
  const pkg = path.join(appDir, "node_modules", "electron");
  try {
    const rel = readFileSync(path.join(pkg, "path.txt"), "utf8").trim();
    const bin = path.join(pkg, "dist", rel);
    return existsSync(bin) ? bin : null;
  } catch {
    return null;
  }
}

function pathElectron() {
  const probe = spawnSync("which", ["electron"], { encoding: "utf8", shell: false });
  const found = probe.status === 0 ? probe.stdout.trim() : null;
  return found && existsSync(found) ? found : null;
}

const electron =
  process.env.POC2_ELECTRON ??
  (process.platform === "linux" ? pathElectron() : null) ??
  npmElectron() ??
  "electron";

const args = [appDir, ...process.argv.slice(2)];
const res = spawnSync(electron, args, { stdio: "inherit", shell: false });
if (res.error) {
  console.error(
    `failed to launch electron (${electron}): ${res.error.message}\n` +
      "Hints: nix develop (NixOS) provides `electron`; elsewhere run " +
      "`bun pm trust electron && bun install` to fetch the npm binary, or set POC2_ELECTRON.",
  );
  process.exit(1);
}
process.exit(res.status ?? 0);
