#!/usr/bin/env bun
// Build the poc2-wasm engine into the Next.js web app (cross-platform).
//   cargo build (wasm32) → wasm-bindgen (JS glue) → wasm-opt (size).
// Tools come from the nix dev shell or rustup + `cargo binstall
// wasm-bindgen-cli` + binaryen. Runs under Bun on Linux/macOS/Windows.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outDir = path.join(repoRoot, "apps", "web", "lib", "wasm");
const publicDir = path.join(repoRoot, "apps", "web", "public", "wasm");
const wasmIn = path.join(
  repoRoot,
  "target",
  "wasm32-unknown-unknown",
  "release",
  "poc2_wasm.wasm",
);

const HINTS = {
  cargo: "install Rust via rustup (https://rustup.rs); wasm32 target comes from rust-toolchain.toml",
  "wasm-bindgen": "cargo binstall wasm-bindgen-cli   (or: cargo install wasm-bindgen-cli)",
  "wasm-opt": "install binaryen (nix/brew/scoop/apt package `binaryen`)",
};

// shell:false everywhere — args must never pass through a platform shell.
function run(tool, args, { fatal = true } = {}) {
  console.log(`▸ ${tool} ${args.join(" ")}`);
  const res = spawnSync(tool, args, { cwd: repoRoot, stdio: "inherit", shell: false });
  if (res.error?.code === "ENOENT") {
    const msg = `error: \`${tool}\` not found on PATH — ${HINTS[tool]}`;
    if (!fatal) {
      console.warn(`  ${msg} (skipped, non-fatal)`);
      return false;
    }
    console.error(msg);
    process.exit(1);
  }
  if (res.error || res.status !== 0) {
    if (!fatal) {
      console.warn(`  ${tool} failed (skipped, non-fatal)`);
      return false;
    }
    console.error(`error: ${tool} exited with status ${res.status ?? res.error}`);
    process.exit(1);
  }
  return true;
}

run("cargo", ["build", "-p", "poc2-wasm", "--target", "wasm32-unknown-unknown", "--release"]);

mkdirSync(outDir, { recursive: true });
run("wasm-bindgen", [
  wasmIn,
  "--out-dir",
  outDir,
  "--target",
  "web",
  "--omit-default-module-path",
]);

// -all enables the wasm features rustc emits; wasm-opt is best-effort.
const bgWasm = path.join(outDir, "poc2_wasm_bg.wasm");
const optWasm = path.join(outDir, "poc2_wasm_bg.opt.wasm");
if (run("wasm-opt", ["-Oz", "-all", bgWasm, "-o", optWasm], { fatal: false })) {
  renameSync(optWasm, bgWasm);
  console.log("  optimized.");
} else {
  rmSync(optWasm, { force: true });
}

// Copy the wasm next to the public dir too, so the worker can fetch it by URL.
mkdirSync(publicDir, { recursive: true });
copyFileSync(bgWasm, path.join(publicDir, "poc2_wasm_bg.wasm"));

const kib = Math.round(statSync(bgWasm).size / 1024);
console.log(`✓ wasm built → ${outDir}  (${kib} KiB)`);
