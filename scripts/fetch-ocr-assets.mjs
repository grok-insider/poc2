#!/usr/bin/env bun
// Vendor the Tesseract.js OCR runtime into the web app's public/ocr/ so the
// renderer-side price-region scan (ADR-0013) works fully **offline** and
// **origin-relative** — it must survive `output: 'export'` and being served
// over the desktop shell's privileged `app://` scheme (no CDN, no localhost).
//
// What lands in apps/web/public/ocr/ (all origin-relative as /ocr/...):
//   - worker.min.js                     ← tesseract.js dist (the worker thread)
//   - tesseract-core-simd.wasm.js       ← tesseract.js-core (SIMD, self-contained)
//   - tesseract-core-simd-lstm.wasm.js  ← SIMD + lstmOnly variant
//   - tesseract-core.wasm.js            ← non-SIMD fallback
//   - tesseract-core-lstm.wasm.js       ← non-SIMD + lstmOnly fallback
//   - best/eng.traineddata.gz           ← accurate English model (~12 MB)
//   - fast/eng.traineddata.gz           ← reward-overlay fast model (~2 MB)
//
// The core `*.wasm.js` files embed the WASM binary (Emscripten single-file
// build), so there is no separate `.wasm` to fetch at runtime — one less
// origin-relative path to get wrong.
//
// The trained-data blob (eng.traineddata.gz) is large, so it is NOT committed
// (see apps/web/.gitignore); it is fetched here from the canonical tessdata
// mirror that tesseract.js itself defaults to (`4.0.0_best`). Override the
// source with OCR_TESSDATA_URL if you mirror it internally. Everything else is
// copied straight out of node_modules, so run `bun install` first.
//
// Cross-platform (Bun on Linux/macOS/Windows): no shell, no bash-isms.
import { copyFileSync, existsSync, mkdirSync, statSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const webRoot = path.join(repoRoot, "apps", "web");
const outDir = path.join(webRoot, "public", "ocr");
// Resolve from the web workspace — `tesseract.js` is a dep of apps/web, and
// Bun installs it into apps/web/node_modules (symlinked into the hoist store).
const require = createRequire(path.join(webRoot, "package.json"));

const TESSDATA_BEST_URL =
  process.env.OCR_TESSDATA_URL ??
  "https://tessdata.projectnaptha.com/4.0.0_best/eng.traineddata.gz";
const TESSDATA_FAST_URL =
  process.env.OCR_TESSDATA_FAST_URL ??
  "https://tessdata.projectnaptha.com/4.0.0_fast/eng.traineddata.gz";

function copyInto(srcAbs, name) {
  const dest = path.join(outDir, name);
  copyFileSync(srcAbs, dest);
  const kb = (statSync(dest).size / 1024).toFixed(0);
  console.log(`  ✓ ${name} (${kb} KB)`);
}

async function main() {
  mkdirSync(outDir, { recursive: true });

  // Resolve the installed packages by their package.json (the `main` entry
  // points at src/, so we anchor on the package dir instead). tesseract.js-core
  // is a transitive dep, so resolve it *through* tesseract.js's own require.
  const tjsPkg = require.resolve("tesseract.js/package.json");
  const tjsDir = path.dirname(tjsPkg);
  const corePkg = createRequire(tjsPkg).resolve("tesseract.js-core/package.json");
  const coreDir = path.dirname(corePkg);

  // --- tesseract.js worker -------------------------------------------------
  console.log("▸ tesseract.js worker");
  copyInto(path.join(tjsDir, "dist", "worker.min.js"), "worker.min.js");

  // --- tesseract.js-core (self-contained .wasm.js variants) ----------------
  console.log("▸ tesseract.js-core (wasm)");
  // tesseract.js v7 picks a core variant at runtime from the browser's WASM
  // feature detection: relaxed-SIMD → SIMD → plain, each in a normal + lstmOnly
  // flavour. Vendor every variant it might ask for so no path 404s offline.
  for (const f of [
    "tesseract-core-relaxedsimd.wasm.js",
    "tesseract-core-relaxedsimd-lstm.wasm.js",
    "tesseract-core-simd.wasm.js",
    "tesseract-core-simd-lstm.wasm.js",
    "tesseract-core.wasm.js",
    "tesseract-core-lstm.wasm.js",
  ]) {
    copyInto(path.join(coreDir, f), f);
  }

  // --- traineddata models (fetched, not vendored from npm) -----------------
  for (const [model, url] of [
    ["best", TESSDATA_BEST_URL],
    ["fast", TESSDATA_FAST_URL],
  ]) {
    const modelDir = path.join(outDir, model);
    mkdirSync(modelDir, { recursive: true });
    const dataDest = path.join(modelDir, "eng.traineddata.gz");
    if (existsSync(dataDest) && statSync(dataDest).size > 1_000_000) {
      console.log(`▸ ${model}/eng.traineddata.gz (already present, skipping fetch)`);
      continue;
    }
    console.log(`▸ ${model}/eng.traineddata.gz ← ${url}`);
    const res = await fetch(url);
    if (!res.ok) {
      console.error(`error: ${model} traineddata fetch failed (HTTP ${res.status})`);
      process.exit(1);
    }
    const buf = Buffer.from(await res.arrayBuffer());
    if (buf.length < 1_000_000) {
      console.error(`error: traineddata suspiciously small (${buf.length} bytes)`);
      process.exit(1);
    }
    writeFileSync(dataDest, buf);
    console.log(`  ✓ ${model}/eng.traineddata.gz (${(buf.length / 1024 / 1024).toFixed(1)} MB)`);
  }

  console.log(`\nOCR assets ready in ${path.relative(repoRoot, outDir)}/ (served as /ocr/...)`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
