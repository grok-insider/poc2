"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { ImageUp, ScanText } from "lucide-react";
import { useCraft } from "@/lib/store";
import { affixCounts, humanizeId } from "@/lib/format";
import { ItemPopup } from "@/components/ItemPopup";
import {
  loadUniqueIconManifest,
  type UniqueIconManifest,
} from "@/lib/itemArt";
import {
  buildItemView,
  loadUniqueCatalog,
  type UniqueCatalog,
} from "@/lib/itemView";
import type { Item, Rarity } from "@/lib/types";
import styles from "./ItemEditor.module.css";

const RARITIES: Rarity[] = ["normal", "magic", "rare", "unique"];

/** A fresh Normal ilvl 82 BodyArmour — the "blank slate" preset. */
function freshNormalArmour(): Item {
  return {
    base: "BodyArmour",
    ilvl: 82,
    rarity: "normal",
    corrupted: false,
    sanctified: false,
    mirrored: false,
    quality: 0,
    quality_kind: "Untagged",
    implicits: [],
    prefixes: [],
    suffixes: [],
    enchantments: [],
    hidden_desecrated: null,
    sockets: [],
    hinekora_lock: null,
  };
}

function clampInt(raw: string, min: number, max: number, fallback: number): number {
  const n = Number.parseInt(raw, 10);
  if (Number.isNaN(n)) return fallback;
  return Math.min(max, Math.max(min, n));
}

export function ItemEditor() {
  const item = useCraft((s) => s.item);
  const setItem = useCraft((s) => s.setItem);
  const loadFixture = useCraft((s) => s.loadFixture);
  const importText = useCraft((s) => s.importText);
  const goal = useCraft((s) => s.goal);
  const setSection = useCraft((s) => s.setSection);
  const lastParse = useCraft((s) => s.lastParse);
  const lastItemText = useCraft((s) => s.lastItemText);
  const iconManifest = useCraft((s) => s.iconManifest);
  // Store-held so captured imports (desktop bridge) surface them too.
  const unresolved = useCraft((s) => s.lastUnresolved);
  const clearImport = useCraft((s) => s.clearImport);

  const [pasted, setPasted] = useState("");
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrNote, setOcrNote] = useState<string[]>([]);
  const [uniqueManifest, setUniqueManifest] = useState<UniqueIconManifest | null>(null);
  const [uniqueCatalog, setUniqueCatalog] = useState<UniqueCatalog | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);

  // The parse preview renders for any import — pasted here or captured.
  const imported = lastParse !== null;

  useEffect(() => {
    void loadUniqueIconManifest().then(setUniqueManifest);
    void loadUniqueCatalog().then(setUniqueCatalog);
  }, []);

  const previewView = useMemo(() => {
    if (!imported || !lastParse || !lastItemText) return null;
    return buildItemView(lastItemText, {
      baseManifest: iconManifest,
      uniqueManifest,
      uniqueCatalog,
    });
  }, [imported, lastParse, lastItemText, iconManifest, uniqueManifest, uniqueCatalog]);

  const counts = affixCounts(item);

  /** Commit a partial change onto the current item (auto-replans via store). */
  function patch(next: Partial<Item>) {
    setItem({ ...item, ...next });
  }

  async function runImport(text: string) {
    const trimmed = text.trim();
    if (!trimmed) return;
    setImporting(true);
    setImportError(null);
    try {
      await importText(trimmed);
      // Lead the user to pick a target — but only when they haven't set one,
      // so re-importing mid-craft doesn't yank them away.
      const hasTarget =
        (goal.target.prefixes ?? []).length + (goal.target.suffixes ?? []).length > 0;
      if (!hasTarget) setSection("target");
    } catch (e) {
      setImportError(String(e));
    } finally {
      setImporting(false);
    }
  }

  async function pasteFromClipboard() {
    setImportError(null);
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        setImportError("Clipboard is empty.");
        return;
      }
      setPasted(text);
      await runImport(text);
    } catch {
      setImportError("Clipboard read was blocked — paste into the box below instead.");
    }
  }

  /** Screenshot OCR fallback (paste an image or pick a file). */
  async function runOcr(file: Blob) {
    setOcrBusy(true);
    setImportError(null);
    setOcrNote([]);
    try {
      const { ocrImageToItemText } = await import("@/lib/ocr");
      const res = await ocrImageToItemText(file, iconManifest);
      setOcrNote(res.warnings);
      if (!res.text) return;
      setPasted(res.text);
      await runImport(res.text);
    } catch (e) {
      setImportError(`OCR failed: ${String(e)}`);
    } finally {
      setOcrBusy(false);
    }
  }

  /** Image paste anywhere in the import block triggers OCR. */
  function onPasteCapture(e: React.ClipboardEvent) {
    const img = Array.from(e.clipboardData?.files ?? []).find((f) =>
      f.type.startsWith("image/"),
    );
    if (img) {
      e.preventDefault();
      void runOcr(img);
    }
  }

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Item</div>
        <span className="faint num">
          {counts.prefix.used + counts.suffix.used} mods
        </span>
      </div>

      <div className="pane-scroll">
        <div className={styles.stack}>
          {/* ---- 1. Import --------------------------------------------- */}
          <section className={`card ${styles.block}`} onPaste={onPasteCapture}>
            <div className={styles.blockHead}>
              <span className="poe-section">Import</span>
              <span className="faint">
                Ctrl+C in game, then paste here — or paste a screenshot.
              </span>
            </div>

            <div className={styles.importRow}>
              <button
                className="btn btn-primary"
                onClick={() => void pasteFromClipboard()}
                disabled={importing || ocrBusy}
              >
                Paste from clipboard
              </button>
              <button
                className="btn"
                onClick={() => void runImport(pasted)}
                disabled={importing || ocrBusy || !pasted.trim()}
              >
                Parse pasted text
              </button>
              <button
                className="btn"
                onClick={() => fileInput.current?.click()}
                disabled={importing || ocrBusy}
                title="Read an item tooltip from a screenshot (OCR, best-effort)"
              >
                <ImageUp size={14} />
                Screenshot…
              </button>
              <input
                ref={fileInput}
                type="file"
                accept="image/*"
                hidden
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) void runOcr(f);
                  e.target.value = "";
                }}
              />
            </div>

            <textarea
              className={`field ${styles.paste}`}
              placeholder={
                "Paste item text here…\n(or paste a tooltip screenshot anywhere in this panel for OCR)"
              }
              value={pasted}
              onChange={(e) => setPasted(e.target.value)}
              rows={4}
              spellCheck={false}
            />

            {(importing || ocrBusy) && (
              <div className={styles.busy}>
                <ScanText size={13} className="faint" />
                <span className="muted">
                  {ocrBusy ? "Reading screenshot (OCR)…" : "Parsing…"}
                </span>
              </div>
            )}

            {importError && (
              <div className={styles.warnRow}>
                <span className="chip danger">{importError}</span>
              </div>
            )}
            {ocrNote.map((w) => (
              <div key={w} className={styles.warnRow}>
                <span className="chip" title={w}>
                  {w}
                </span>
              </div>
            ))}

            {/* ---- Full parse preview: poe2db-style item popup + art ---- */}
            {imported && lastParse && previewView && (
              <div className={styles.preview}>
                <ItemPopup model={previewView.model} artUrl={previewView.artUrl} />
                {previewView.uniqueMatched && (
                  <div className={styles.previewMeta}>
                    Unique matched from catalog
                    {previewView.uniqueKey ? ` · ${previewView.uniqueKey}` : ""}
                  </div>
                )}
                {!lastParse.baseResolved && (
                  <div className={styles.previewUnresolved}>
                    Base not resolved — the modifier pool is approximate.
                  </div>
                )}
                {lastParse.warnings.map((w, i) => (
                  <div key={`w${i}`} className={styles.previewUnresolved} title={w}>
                    {w}
                  </div>
                ))}
              </div>
            )}
          </section>

          {/* ---- 2. Base ---------------------------------------------- */}
          <section className={`card ${styles.block}`}>
            <div className={styles.blockHead}>
              <span className="eyebrow">Base</span>
              {item.base_display_name && (
                <span className={`${styles.gold} gold`}>{item.base_display_name}</span>
              )}
            </div>

            <label className="field-row">
              <span className="field-label">Class id</span>
              <input
                className="field mono"
                value={item.base}
                placeholder="BodyArmour"
                spellCheck={false}
                onChange={(e) => patch({ base: e.target.value })}
              />
            </label>

            <div className={styles.grid2}>
              <label className="field-row">
                <span className="field-label">Item level</span>
                <input
                  className="field num"
                  type="number"
                  min={1}
                  max={100}
                  value={item.ilvl}
                  onChange={(e) => patch({ ilvl: clampInt(e.target.value, 1, 100, item.ilvl) })}
                />
              </label>
              <label className="field-row">
                <span className="field-label">Quality %</span>
                <input
                  className="field num"
                  type="number"
                  min={0}
                  max={30}
                  value={item.quality}
                  onChange={(e) =>
                    patch({ quality: clampInt(e.target.value, 0, 30, item.quality) })
                  }
                />
              </label>
            </div>

            <div className="field-row">
              <span className="field-label">Rarity</span>
              <div className="seg">
                {RARITIES.map((r) => (
                  <button
                    key={r}
                    className={item.rarity === r ? "on" : ""}
                    onClick={() => patch({ rarity: r })}
                  >
                    <span className={item.rarity === r ? `r-${r}` : undefined}>{r}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className={styles.flags}>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={item.corrupted}
                  onChange={(e) => patch({ corrupted: e.target.checked })}
                />
                <span className={item.corrupted ? "r-corrupted" : undefined}>Corrupted</span>
              </label>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={item.mirrored}
                  onChange={(e) => patch({ mirrored: e.target.checked })}
                />
                <span>Mirrored</span>
              </label>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={item.sanctified}
                  onChange={(e) => patch({ sanctified: e.target.checked })}
                />
                <span className={item.sanctified ? "r-desecrated" : undefined}>Sanctified</span>
              </label>
            </div>

            <div className={styles.capacity}>
              <span className="faint">Capacity</span>
              <span className="num muted">
                {counts.prefix.used}/{counts.prefix.max} prefix
              </span>
              <span className="faint">·</span>
              <span className="num muted">
                {counts.suffix.used}/{counts.suffix.max} suffix
              </span>
            </div>
          </section>

          {/* ---- 3. Presets ------------------------------------------- */}
          <section className={`card ${styles.block}`}>
            <div className={styles.blockHead}>
              <span className="eyebrow">Presets</span>
              <span className="faint">Reset to a known starting state.</span>
            </div>
            <div className={styles.importRow}>
              <button className="btn btn-gold" onClick={() => loadFixture()}>
                Worked example
              </button>
              <button
                className="btn"
                onClick={() => {
                  clearImport();
                  setImportError(null);
                  setItem(freshNormalArmour());
                }}
              >
                Fresh Normal armour
              </button>
            </div>
            <span className="faint">
              Fresh: ilvl 82 Normal{" "}
              <span className="mono">{humanizeId("BodyArmour")}</span>, no mods.
            </span>
          </section>
        </div>
      </div>
    </div>
  );
}
