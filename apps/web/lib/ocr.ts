"use client";

/// Screenshot → item-text OCR (best-effort import fallback).
///
/// Research note (see CHANGELOG): desktop overlays (Awakened PoE Trade,
/// Exiled Exchange 2) capture items via simulated Ctrl+C → clipboard — OCR
/// appears only in their Heist tooling. The browser can't inject keystrokes,
/// so in-game **Ctrl+C remains the lossless path**; this module covers the
/// screenshot case (console players, second screen, cropped images).
///
/// Pipeline, modeled on the studied implementations:
///  1. Canvas preprocess: 3× upscale, max-channel luminance, threshold,
///     invert → black text on white (Tesseract's training domain).
///  2. tesseract.js (lazy-loaded), PSM 6, character whitelist.
///  3. Line cleanup + fuzzy base-name match against the base-icon manifest
///     (bigram similarity — the "dictionary correction" trick that makes
///     OCR errors irrelevant for the base line).
///  4. Reconstruct PoE2 clipboard text and hand it to the normal parser;
///     unresolved lines surface in the import preview for manual fixes.

import type { BaseIconManifest } from "./types";

export interface OcrImportResult {
  /** Reconstructed clipboard-style text (feed to `importText`). */
  text: string;
  /** Raw OCR lines, for the preview/debugging. */
  rawLines: string[];
  /** Matched base, when found. */
  base: { id: string; name: string; classPascal: string } | null;
  /** Similarity score of the base match (0..1). */
  baseScore: number;
  warnings: string[];
}

/** Dice coefficient over character bigrams — cheap fuzzy similarity. */
export function bigramSimilarity(a: string, b: string): number {
  const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
  const x = norm(a);
  const y = norm(b);
  if (x.length < 2 || y.length < 2) return x === y ? 1 : 0;
  const grams = new Map<string, number>();
  for (let i = 0; i < x.length - 1; i++) {
    const g = x.slice(i, i + 2);
    grams.set(g, (grams.get(g) ?? 0) + 1);
  }
  let hits = 0;
  for (let i = 0; i < y.length - 1; i++) {
    const g = y.slice(i, i + 2);
    const n = grams.get(g) ?? 0;
    if (n > 0) {
      grams.set(g, n - 1);
      hits++;
    }
  }
  return (2 * hits) / (x.length + y.length - 2);
}

/** Upscale + binarize + invert for OCR. Returns a data-URL PNG. */
async function preprocess(file: Blob): Promise<string> {
  const bmp = await createImageBitmap(file);
  // 3× target for small crops; cap the long edge at ~3200px for huge shots.
  const scale = Math.max(1.5, Math.min(3, 3200 / Math.max(1, bmp.width)));
  const canvas = document.createElement("canvas");
  canvas.width = Math.round(bmp.width * scale);
  canvas.height = Math.round(bmp.height * scale);
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("canvas 2d context unavailable");
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
  const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const d = img.data;
  // max-channel luminance keeps colored mod text (blue/gold) legible where
  // plain grayscale would crush it; threshold + invert → black on white.
  for (let i = 0; i < d.length; i += 4) {
    const lum = Math.max(d[i], d[i + 1], d[i + 2]);
    const v = lum > 72 ? 0 : 255;
    d[i] = v;
    d[i + 1] = v;
    d[i + 2] = v;
    d[i + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  return canvas.toDataURL("image/png");
}

const PROPERTY_PREFIXES = [
  "armour",
  "evasion",
  "energy shield",
  "block chance",
  "ward",
  "spirit",
  "quality",
  "physical damage",
  "elemental damage",
  "critical hit chance",
  "attacks per second",
  "weapon range",
  "reload time",
  "requires",
  "item level",
  "sockets",
  "grants skill",
  "stack size",
];

const NOISE = [/^alt\b/i, /^shift\b/i, /inspect/i, /compare/i, /price check/i];

/** Normalize raw OCR output into candidate tooltip lines (drop UI noise). */
export function cleanLines(raw: string): string[] {
  return raw
    .split("\n")
    .map((l) => l.replace(/[|]/g, "I").replace(/\s+/g, " ").trim())
    .filter((l) => {
      const alnum = l.replace(/[^a-zA-Z0-9]/g, "");
      if (alnum.length < 3) return false;
      return !NOISE.some((re) => re.test(l));
    });
}

/**
 * OCR a pasted screenshot and reconstruct PoE2 clipboard text.
 *
 * Uses the same vendored, origin-relative `/ocr/` runtime as the price
 * overlay (`lib/ocr/tesseract.ts`) — one Tesseract setup for the whole
 * app, no CDN fetches, and it works over the desktop `app://` scheme.
 */
export async function ocrImageToItemText(
  file: Blob,
  manifest: BaseIconManifest | null,
): Promise<OcrImportResult> {
  const warnings: string[] = [];
  const dataUrl = await preprocess(file);

  const { createOcrWorker } = await import("./ocr/tesseract");
  const worker = await createOcrWorker({
    psm: "6",
    charWhitelist:
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 %+-,.':()/",
  });
  try {
    const { data } = await worker.recognize(dataUrl);
    const lines = cleanLines(data.text ?? "");
    if (lines.length === 0) {
      return {
        text: "",
        rawLines: [],
        base: null,
        baseScore: 0,
        warnings: ["No text recognised — try a tighter crop of the tooltip."],
      };
    }

    // Fuzzy-match the base name among the first lines (name lines come first
    // in a tooltip; the base line may be line 1 (normal/magic) or 2 (rare)).
    let best: { id: string; name: string; classPascal: string } | null = null;
    let bestScore = 0;
    let bestLineIdx = -1;
    if (manifest) {
      const head = lines.slice(0, 6);
      for (const [id, entry] of Object.entries(manifest.entries)) {
        for (let i = 0; i < head.length; i++) {
          const s = bigramSimilarity(head[i], entry.name);
          if (s > bestScore) {
            bestScore = s;
            bestLineIdx = i;
            best = { id, name: entry.name, classPascal: entry.class_pascal };
          }
        }
      }
    }
    if (!best || bestScore < 0.55) {
      warnings.push(
        best
          ? `Base guess "${best.name}" is low-confidence (${Math.round(bestScore * 100)}%).`
          : "No base item matched — check the screenshot crop.",
      );
      if (bestScore < 0.4) best = null;
    }

    // Rarity guess: a distinct name line above the base line ⇒ Rare.
    const nameLine =
      best && bestLineIdx > 0 && bigramSimilarity(lines[bestLineIdx - 1], best.name) < 0.7
        ? lines[bestLineIdx - 1]
        : null;
    const rarity = nameLine ? "Rare" : "Normal";

    // ilvl: prefer an explicit Item Level row; else the Requires level
    // (floor approximation, flagged).
    let ilvl = 0;
    for (const l of lines) {
      const mIlvl = /item level:?\s*(\d+)/i.exec(l);
      if (mIlvl) {
        ilvl = Number(mIlvl[1]);
        break;
      }
    }
    if (ilvl === 0) {
      for (const l of lines) {
        const mReq = /requires:?\s*level\s*(\d+)/i.exec(l);
        if (mReq) {
          ilvl = Number(mReq[1]);
          warnings.push(
            `Item Level not visible — using Requires Level ${ilvl} as a floor (edit if needed).`,
          );
          break;
        }
      }
    }
    if (ilvl === 0) {
      ilvl = 1;
      warnings.push("No level information found — defaulted to ilvl 1.");
    }

    // Candidate mod lines: after the base line, skip property rows and
    // class-name echoes ("Boots", "Body Armour" under the title).
    const startIdx = best ? bestLineIdx + 1 : 0;
    const classWords = best
      ? best.classPascal.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase()
      : "";
    const modLines = lines.slice(startIdx).filter((l) => {
      const low = l.toLowerCase();
      if (PROPERTY_PREFIXES.some((p) => low.startsWith(p))) return false;
      if (classWords && bigramSimilarity(low, classWords) > 0.8) return false;
      if (best && bigramSimilarity(l, best.name) > 0.8) return false;
      return true;
    });

    const parts: string[] = [];
    parts.push(`Item Class: ${best ? best.classPascal : "BodyArmour"}`);
    parts.push(`Rarity: ${rarity}`);
    if (nameLine) parts.push(nameLine);
    parts.push(best ? best.name : lines[0]);
    parts.push("--------");
    parts.push(`Item Level: ${ilvl}`);
    if (modLines.length > 0) {
      parts.push("--------");
      parts.push(...modLines);
    }

    return {
      text: parts.join("\n"),
      rawLines: lines,
      base: best,
      baseScore: bestScore,
      warnings,
    };
  } finally {
    await worker.terminate();
  }
}
