#!/usr/bin/env bun
// Regenerate the crates/market/data/locales/<code>.json datasets from poe2db.
//
// Clean-room provenance: this fetches GGG's *own* localized item names from
// poe2db.tw's per-language pages and emits an `English -> localized` map. It
// is intentionally a SEPARATE, human-run step — the committed datasets are a
// curated starter set, and any scraped expansion must be reviewed before
// commit (per AGENTS.md: generated/scraped data needs human curation).
//
// The matcher (crates/market/src/name_match.rs) inverts these into a
// localized->English lookup at load time, so the files stay English-keyed for
// clean diffs and obvious gaps.
//
// Usage:
//   bun run scripts/regen-locales.mjs --code de --dry-run
//   bun run scripts/regen-locales.mjs --code de --out crates/market/data/locales/de.json
//
// Status: SCAFFOLD. The fetch/parse against poe2db is left as a deliberate
// manual step (network + curation). It prints the contract and merges into the
// existing curated file rather than overwriting it, so a partial scrape can
// never silently drop curated entries.

import { readFileSync, writeFileSync, existsSync } from "node:fs";

const LANGS = {
  de: { language: "German", poe2db: "https://poe2db.tw/de/" },
  fr: { language: "French", poe2db: "https://poe2db.tw/fr/" },
  pt: { language: "Portuguese", poe2db: "https://poe2db.tw/pt/" },
  ru: { language: "Russian", poe2db: "https://poe2db.tw/ru/" },
  sp: { language: "Spanish", poe2db: "https://poe2db.tw/es/" },
};

function arg(name, fallback = undefined) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1) return fallback;
  const next = process.argv[i + 1];
  return next && !next.startsWith("--") ? next : true;
}

const code = arg("code");
const dryRun = arg("dry-run", false) === true;
if (!code || !LANGS[code]) {
  console.error(`--code must be one of: ${Object.keys(LANGS).join(", ")}`);
  process.exit(2);
}

const meta = LANGS[code];
const outPath = arg("out", `crates/market/data/locales/${code}.json`);

// Load the existing curated file (we MERGE, never blindly overwrite).
let existing = { entries: {} };
if (existsSync(outPath)) {
  existing = JSON.parse(readFileSync(outPath, "utf8"));
}

console.error(`[regen-locales] ${code} (${meta.language})`);
console.error(`  source page : ${meta.poe2db}`);
console.error(`  out file    : ${outPath}`);
console.error(`  curated now : ${Object.keys(existing.entries ?? {}).length} entries`);
console.error("");
console.error("  This is a scaffold. To expand the dataset:");
console.error("   1. Fetch the localized currency/rune index pages from poe2db.");
console.error("   2. Pair each English canonical name with its localized name.");
console.error("   3. Merge into entries below (English-keyed), then HUMAN-REVIEW.");
console.error("   4. Keep LF line endings (.gitattributes enforces eol=lf).");

// --- scrape hook (left unimplemented on purpose) -------------------------
// const scraped = await scrapePoe2db(meta.poe2db);  // { English: localized }
const scraped = {};

const merged = {
  language: existing.language ?? meta.language,
  code,
  source: Object.keys(scraped).length ? "poe2db+curated" : (existing.source ?? "curated-starter"),
  note: existing.note ?? "Starter set; expand via scripts/regen-locales.mjs with human review.",
  // Curated entries win over scraped on conflict (manual review is trusted).
  entries: { ...scraped, ...(existing.entries ?? {}) },
};

const json = JSON.stringify(merged, null, 2) + "\n";
if (dryRun) {
  console.error("\n[dry-run] would write:\n");
  process.stdout.write(json);
} else {
  writeFileSync(outPath, json);
  console.error(`\n[regen-locales] wrote ${Object.keys(merged.entries).length} entries → ${outPath}`);
}
