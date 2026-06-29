"use client";

import { useRef, useState } from "react";
import { ImageUp, ScanText } from "lucide-react";
import { useCraft } from "@/lib/store";
import { affixCounts, humanizeId, humanizeModId, modValue } from "@/lib/format";
import { BaseIcon } from "@/components/BaseIcon";
import type { Item, ModRoll, Rarity } from "@/lib/types";
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

/** One blue mod line in the parse preview. */
function PreviewMod({ m, side }: { m: ModRoll; side?: "P" | "S" }) {
  const v = modValue(m);
  return (
    <div className={styles.previewMod}>
      {side && <span className={styles.previewSide}>{side}</span>}
      <span className={m.kind === "crafted" ? "r-crafted" : "poe-pop-mod"}>
        {humanizeModId(m.mod_id)}
        {v && <span className="num"> {v}</span>}
      </span>
      {m.is_fractured && <span className="r-fractured"> (fractured)</span>}
    </div>
  );
}

export function ItemEditor() {
  const item = useCraft((s) => s.item);
  const setItem = useCraft((s) => s.setItem);
  const loadFixture = useCraft((s) => s.loadFixture);
  const importText = useCraft((s) => s.importText);
  const goal = useCraft((s) => s.goal);
  const setSection = useCraft((s) => s.setSection);
  const lastParse = useCraft((s) => s.lastParse);
  const iconManifest = useCraft((s) => s.iconManifest);
  // Store-held so captured imports (desktop bridge) surface them too.
  const unresolved = useCraft((s) => s.lastUnresolved);
  const clearImport = useCraft((s) => s.clearImport);

  const [pasted, setPasted] = useState("");
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrNote, setOcrNote] = useState<string[]>([]);
  const fileInput = useRef<HTMLInputElement | null>(null);

  // The parse preview renders for any import — pasted here or captured.
  const imported = lastParse !== null;

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

            {/* ---- Full parse preview: the whole item, PoE2-style ------- */}
            {imported && lastParse && (
              <div
                className={`poe-pop ${item.rarity === "rare" ? "poe-pop--rare" : item.rarity === "magic" ? "poe-pop--magic" : ""} ${styles.preview}`}
              >
                <div className="poe-pop-header">
                  <span>{item.base_display_name ?? humanizeId(item.base)}</span>
                </div>
                <div className="poe-pop-content">
                  <div className={styles.previewBaseRow}>
                    <BaseIcon
                      baseId={item.base_type_id ?? item.base}
                      name={item.base_display_name ?? humanizeId(item.base)}
                      size={40}
                    />
                    <div>
                      {lastParse.itemClassId
                        ? humanizeId(lastParse.itemClassId)
                        : humanizeId(item.base)}
                      <span className="faint"> · </span>
                      <span className={`r-${item.rarity}`}>{item.rarity}</span>
                      <div>
                        Item Level: <span className={styles.previewValue}>{item.ilvl}</span>
                        {item.quality > 0 && (
                          <>
                            <span className="faint"> · </span>Quality:{" "}
                            <span className="poe-pop-mod">+{item.quality}%</span>
                          </>
                        )}
                      </div>
                    </div>
                  </div>

                  {item.implicits.length > 0 && (
                    <>
                      <div className="poe-pop-sep" />
                      {item.implicits.map((m, i) => (
                        <PreviewMod key={`i${i}`} m={m} />
                      ))}
                    </>
                  )}

                  <div className="poe-pop-sep" />
                  {item.prefixes.length === 0 && item.suffixes.length === 0 ? (
                    <div className="poe-pop-note">No explicit modifiers.</div>
                  ) : (
                    <>
                      {item.prefixes.map((m, i) => (
                        <PreviewMod key={`p${i}`} m={m} side="P" />
                      ))}
                      {item.suffixes.map((m, i) => (
                        <PreviewMod key={`s${i}`} m={m} side="S" />
                      ))}
                    </>
                  )}

                  {unresolved.length > 0 && (
                    <>
                      <div className="poe-pop-sep" />
                      <div className={styles.previewUnresolvedHead}>
                        {unresolved.length} line{unresolved.length === 1 ? "" : "s"} not
                        recognised as modifiers:
                      </div>
                      {unresolved.map((line, i) => (
                        <div key={i} className={styles.previewUnresolved} title={line}>
                          {line}
                        </div>
                      ))}
                    </>
                  )}

                  {(item.corrupted || item.sanctified || item.mirrored) && (
                    <>
                      <div className="poe-pop-sep" />
                      {item.corrupted && <div className="r-corrupted">Corrupted</div>}
                      {item.sanctified && <div className="r-desecrated">Sanctified</div>}
                      {item.mirrored && <div className="poe-pop-note">Mirrored</div>}
                    </>
                  )}

                  {!lastParse.baseResolved && (
                    <>
                      <div className="poe-pop-sep" />
                      <div className={styles.previewUnresolved}>
                        Base not resolved — the modifier pool is approximate.
                      </div>
                    </>
                  )}
                  {lastParse.warnings.map((w, i) => (
                    <div key={`w${i}`} className={styles.previewUnresolved} title={w}>
                      {w}
                    </div>
                  ))}
                </div>
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
