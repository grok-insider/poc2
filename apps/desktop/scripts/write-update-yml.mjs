#!/usr/bin/env node
/**
 * Fallback generator for electron-updater feed files when electron-builder
 * does not emit latest.yml / latest-linux.yml (seen with builder 26 +
 * `--publish never` in some configs).
 *
 * Usage (from apps/desktop or repo root):
 *   node scripts/write-update-yml.mjs --dir dist-packages --version 2.0.0 --platform linux
 *   node scripts/write-update-yml.mjs --dir dist-packages --version 2.0.0 --platform windows
 *
 * Matches the YAML shape electron-updater expects for GitHub Releases:
 * version, files[{url,sha512,size}], path, sha512, releaseDate.
 */
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";

function arg(name, fallback = undefined) {
  const i = process.argv.indexOf(`--${name}`);
  if (i === -1) return fallback;
  return process.argv[i + 1] ?? fallback;
}

function sha512File(filePath) {
  const h = createHash("sha512");
  h.update(readFileSync(filePath));
  return h.digest("base64");
}

function pickArtifact(dir, platform) {
  const names = readdirSync(dir);
  if (platform === "windows") {
    const exe = names.find((n) => n.endsWith(".exe") && !n.endsWith(".blockmap"));
    if (!exe) throw new Error(`no .exe in ${dir}`);
    return { file: exe, yml: "latest.yml" };
  }
  if (platform === "linux") {
    // Prefer AppImage (auto-update target); fall back to first .deb.
    const appImage = names.find((n) => n.endsWith(".AppImage"));
    const deb = names.find((n) => n.endsWith(".deb"));
    const file = appImage ?? deb;
    if (!file) throw new Error(`no .AppImage or .deb in ${dir}`);
    return { file, yml: "latest-linux.yml" };
  }
  throw new Error(`platform must be windows|linux, got ${platform}`);
}

function main() {
  const dir = path.resolve(arg("dir", "dist-packages"));
  const version = arg("version");
  const platform = arg("platform");
  if (!version || !platform) {
    console.error(
      "usage: write-update-yml.mjs --dir <out> --version <semver> --platform windows|linux",
    );
    process.exit(2);
  }

  const { file, yml } = pickArtifact(dir, platform);
  const full = path.join(dir, file);
  const size = statSync(full).size;
  const sha512 = sha512File(full);
  const releaseDate = new Date().toISOString();

  // Minimal electron-updater feed (GitHub provider resolves url as asset name).
  const lines = [
    `version: ${version}`,
    `files:`,
    `  - url: ${file}`,
    `    sha512: ${sha512}`,
    `    size: ${size}`,
    `path: ${file}`,
    `sha512: ${sha512}`,
    `releaseDate: '${releaseDate}'`,
    ``,
  ];

  // Optional blockmap entry when present (Windows NSIS delta).
  const blockmap = `${file}.blockmap`;
  if (readdirSync(dir).includes(blockmap)) {
    // electron-updater finds *.blockmap by convention next to the installer;
    // no extra YAML field required for basic full updates.
  }

  const outPath = path.join(dir, yml);
  writeFileSync(outPath, lines.join("\n"), "utf8");
  console.log(`wrote ${outPath} for ${file} (v${version})`);
}

main();
