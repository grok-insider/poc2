"use client";

import { useCallback, useEffect, useState } from "react";
import { RefreshCw, RotateCcw, Trash2, Database, Cpu, Puzzle } from "lucide-react";
import { useCraft } from "@/lib/store";
import { addStoredPlugin, removeStoredPlugin } from "@/lib/plugins/store";
import {
  getDesktopBridge,
  type CaptureRect,
  type DesktopCapabilities,
  type PriceStatus as CacheStatus,
  type ScanDiagnostics,
} from "@/lib/desktop";
import { engine } from "@/lib/engine/client";
import type { PoeScoutCurrencyEntry, PoeScoutSnapshot } from "@/lib/types";
import { MARKET_LEAGUE_PRESETS } from "@/lib/marketLeagues";
import styles from "./SettingsPanel.module.css";

type PriceStatus = { kind: "ok"; note: string } | { kind: "err" } | null;

// poe2scout endpoints + categories mirror the native poller
// (crates/market/src/prices.rs `fetch_snapshot`); the browser assembles the
// same snapshot JSON and hands it to the WASM engine's `applyPrices`.
const POE2SCOUT_BASE = "https://poe2scout.com/api/poe2";
const POE2SCOUT_CATEGORIES = ["currency", "essences", "ritual", "abyss", "breach"];

/** League metadata from `/Leagues` (PascalCase, straight off the API). */
interface PoeScoutLeague {
  Value: string;
  /** Exalts per divine. */
  DivinePrice: number;
  /** Chaos per divine. */
  ChaosDivinePrice: number;
}

/** One `/Currencies/ByCategory` page. */
interface PoeScoutCategoryPage {
  CurrentPage: number;
  Pages: number;
  Total: number;
  Items: PoeScoutCurrencyEntry[];
}

/** GET JSON via the desktop bridge (no CORS) or direct fetch in a browser. */
async function getJson(url: string): Promise<unknown> {
  const bridge = getDesktopBridge();
  if (bridge) return bridge.fetchJson(url);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/** Assemble the poe2scout snapshot the engine's `applyPrices` expects. */
async function fetchScoutSnapshot(league: string): Promise<PoeScoutSnapshot> {
  const leagues = (await getJson(`${POE2SCOUT_BASE}/Leagues`)) as PoeScoutLeague[];
  const found = Array.isArray(leagues) ? leagues.find((l) => l.Value === league) : undefined;
  if (!found) throw new Error(`league ${league} not found on poe2scout`);

  const entries: Record<string, PoeScoutCurrencyEntry> = {};
  for (const category of POE2SCOUT_CATEGORIES) {
    for (let page = 1; ; page += 1) {
      const url =
        `${POE2SCOUT_BASE}/Leagues/${encodeURIComponent(found.Value)}` +
        `/Currencies/ByCategory?Category=${category}&Page=${page}&PerPage=250`;
      const resp = (await getJson(url)) as PoeScoutCategoryPage;
      for (const entry of resp.Items ?? []) entries[entry.ApiId] = entry;
      if (page >= Math.max(resp.Pages ?? 1, 1)) break;
    }
  }

  return {
    league: found.Value,
    divine_price_in_exalts: found.DivinePrice,
    chaos_per_divine: found.ChaosDivinePrice,
    entries,
    fetched_at: new Date().toISOString(),
  };
}

export function SettingsPanel() {
  const league = useCraft((s) => s.league);
  const clientLocale = useCraft((s) => s.clientLocale);
  const setClientLocale = useCraft((s) => s.setClientLocale);
  const setLeague = useCraft((s) => s.setLeague);
  const engineLeague = useCraft((s) => s.engineLeague);
  const setEngineLeague = useCraft((s) => s.setEngineLeague);
  const captureStatus = useCraft((s) => s.captureStatus);
  const captureDaemonVersion = useCraft((s) => s.captureDaemonVersion);
  const captureLastAt = useCraft((s) => s.captureLastAt);
  const captureLastError = useCraft((s) => s.captureLastError);
  const notes = useCraft((s) => s.notes);
  const setNotes = useCraft((s) => s.setNotes);
  const patch = useCraft((s) => s.patch);
  const modCount = useCraft((s) => s.modCount);
  const loadFixture = useCraft((s) => s.loadFixture);
  const clearHistory = useCraft((s) => s.clearHistory);
  const replan = useCraft((s) => s.replan);
  const plugins = useCraft((s) => s.plugins);
  const reloadPlugins = useCraft((s) => s.reloadPlugins);

  async function onAddPlugin(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    const bytes = await file.arrayBuffer();
    await addStoredPlugin(file.name.replace(/\.wasm$/i, ""), bytes);
    await reloadPlugins();
  }

  async function onRemovePlugin(name: string) {
    await removeStoredPlugin(name);
    await reloadPlugins();
  }

  const [priceLoading, setPriceLoading] = useState(false);
  const [priceStatus, setPriceStatus] = useState<PriceStatus>(null);

  // Desktop poe2scout price cache (sqlite, the OCR overlay's price source).
  // Only present in the Electron shell; a plain browser leaves this null.
  const [cacheStatus, setCacheStatus] = useState<CacheStatus | null>(null);
  const [cacheBusy, setCacheBusy] = useState(false);
  const [desktopCaps, setDesktopCaps] = useState<DesktopCapabilities | null>(null);
  const [captureRegion, setCaptureRegion] = useState<CaptureRect | null>(null);
  const [scanDiagnostics, setScanDiagnostics] = useState<ScanDiagnostics | null>(null);
  const [watcherEnabled, setWatcherEnabled] = useState(false);

  const loadCacheStatus = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge?.pricesStatus) return;
    try {
      setCacheStatus(await bridge.pricesStatus());
    } catch {
      /* leave previous status */
    }
  }, []);

  const loadOverlayDiagnostics = useCallback(async () => {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    const [caps, region, scan, watcher] = await Promise.all([
      bridge.capabilities().catch(() => null),
      bridge.getCaptureRegion?.().catch(() => null) ?? Promise.resolve(null),
      bridge.scanDiagnostics().catch(() => null),
      bridge.rewardWatcherStatus().catch(() => false),
    ]);
    setDesktopCaps(caps);
    setCaptureRegion(region);
    setScanDiagnostics(scan);
    setWatcherEnabled(watcher);
  }, []);

  useEffect(() => {
    let alive = true;
    const bridge = getDesktopBridge();
    if (!bridge?.pricesStatus) return;
    bridge
      .pricesStatus()
      .then((s) => {
        if (alive) setCacheStatus(s);
      })
      .catch(() => {});
    queueMicrotask(() => void loadOverlayDiagnostics());
    return () => {
      alive = false;
    };
  }, [league, loadOverlayDiagnostics]);

  async function refreshCache() {
    const bridge = getDesktopBridge();
    if (!bridge?.pricesRefresh) return;
    setCacheBusy(true);
    try {
      await bridge.pricesRefresh();
    } catch {
      /* status reload surfaces lastError */
    } finally {
      await loadCacheStatus();
      setCacheBusy(false);
    }
  }

  async function scanRewardsNow() {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    await bridge.scanRewards();
    window.setTimeout(() => void loadOverlayDiagnostics(), 1_500);
  }

  async function toggleRewardWatcher() {
    const bridge = getDesktopBridge();
    if (!bridge) return;
    setWatcherEnabled(await bridge.rewardWatcher(!watcherEnabled));
    window.setTimeout(() => void loadOverlayDiagnostics(), 500);
  }

  async function refreshPrices() {
    setPriceLoading(true);
    setPriceStatus(null);
    try {
      const snapshot = await fetchScoutSnapshot(league);
      const view = await engine.applyPrices(snapshot);
      setPriceStatus({
        kind: "ok",
        note: `applied ${view.applied} prices (${view.unmatched.length} unmatched)`,
      });
      // Re-plan immediately so visible recommendations use the fresh
      // valuator instead of waiting for the next state change.
      void replan();
    } catch {
      setPriceStatus({ kind: "err" });
    } finally {
      setPriceLoading(false);
    }
  }

  function confirmClear() {
    if (window.confirm("Clear all crafting history? This cannot be undone.")) {
      clearHistory();
    }
  }

  const isPreset = MARKET_LEAGUE_PRESETS.some((preset) => preset.value === league);

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Settings</div>
      </div>

      <div className="pane-scroll">
        <div className={styles.stack}>
          {/* ---- MARKET ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Market</span>
              <button
                className="btn"
                onClick={() => void refreshPrices()}
                disabled={priceLoading}
                title="Best-effort live price fetch from poe2scout"
              >
                <RefreshCw size={13} className={priceLoading ? styles.spin : undefined} />
                {priceLoading ? "Refreshing…" : "Refresh prices"}
              </button>
            </div>

            <div className="field-row">
              <label className="field-label" htmlFor="league-input">
                League
              </label>
              <div className={styles.leagueRow}>
                <div className="seg">
                  {MARKET_LEAGUE_PRESETS.map((preset) => (
                    <button
                      key={preset.value}
                      className={league === preset.value ? "on" : ""}
                      onClick={() => setLeague(preset.value)}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
                <input
                  id="league-input"
                  className={`field ${styles.leagueInput}`}
                  value={isPreset ? "" : league}
                  placeholder="Custom league…"
                  onChange={(e) => setLeague(e.target.value)}
                  spellCheck={false}
                />
              </div>
            </div>

            <div className="field-row">
              <label className="field-label" htmlFor="client-locale">
                Client language
              </label>
              <div className="seg" id="client-locale">
                {(
                  [
                    { value: "en", label: "English" },
                    { value: "sp", label: "Spanish" },
                    { value: "de", label: "German" },
                    { value: "fr", label: "French" },
                    { value: "pt", label: "Portuguese" },
                    { value: "ru", label: "Russian" },
                  ] as const
                ).map((opt) => (
                  <button
                    key={opt.value}
                    type="button"
                    className={clientLocale === opt.value ? "on" : ""}
                    onClick={() => setClientLocale(opt.value)}
                    title="Matches your PoE2 client language so reward-scan OCR maps item names to English prices"
                  >
                    {opt.label}
                  </button>
                ))}
              </div>
            </div>
            <p className={`${styles.note} faint`}>
              Reward scan translates OCR&apos;d item names from this language into English
              catalogue keys (poe2scout stays English). Spanish uses code{" "}
              <span className="num">sp</span>.
            </p>

            {priceStatus?.kind === "ok" && (
              <p className={`${styles.note} success num`}>{priceStatus.note}</p>
            )}
            {priceStatus?.kind === "err" && (
              <p className={`${styles.note} faint`}>
                Browser CORS may block direct price fetches. The advisor runs fully
                without live prices.
              </p>
            )}
            {!priceStatus && (
              <p className={`${styles.note} faint`}>
                Live prices are informational only — planning never depends on them.
              </p>
            )}

            {cacheStatus && (
              <div className={styles.cacheBox}>
                <div className={styles.sectionHead}>
                  <span className="eyebrow">Overlay price cache</span>
                  <button
                    className="btn"
                    onClick={() => void refreshCache()}
                    disabled={cacheBusy || cacheStatus.refreshing}
                    title="Force a poe2scout refresh of the OCR overlay's price cache"
                  >
                    <RefreshCw
                      size={13}
                      className={cacheBusy || cacheStatus.refreshing ? styles.spin : undefined}
                    />
                    {cacheBusy || cacheStatus.refreshing ? "Refreshing…" : "Refresh cache"}
                  </button>
                </div>
                <div className={styles.dataGrid}>
                  <span className="faint">League</span>
                  <span className="num">{cacheStatus.league || "—"}</span>
                  <span className="faint">Priced items</span>
                  <span className="num">{cacheStatus.count.toLocaleString()}</span>
                  <span className="faint">Updated</span>
                  <span className="num">
                    {cacheStatus.fetchedAt
                      ? new Date(cacheStatus.fetchedAt).toLocaleTimeString()
                      : "—"}
                  </span>
                  <span className="faint">Backend</span>
                  <span className="num">{cacheStatus.backend}</span>
                </div>
                {cacheStatus.lastError && (
                  <p className={`${styles.note} danger`}>{cacheStatus.lastError}</p>
                )}
                <p className={`${styles.note} faint`}>
                  The screenshot-OCR overlay (<span className="num">ALT+V</span>) prices
                  currency, runes, idols and omens from this cache. It refreshes hourly and
                  follows the league above.
                </p>
              </div>
            )}

            {desktopCaps && (
              <div className={styles.cacheBox}>
                <div className={styles.sectionHead}>
                  <span className="eyebrow">OCR diagnostics</span>
                  <div className={styles.actionRow}>
                    <button
                      className="btn btn-ghost"
                      onClick={() => void getDesktopBridge()?.calibrateRegion()}
                    >
                      Calibrate
                    </button>
                    <button className="btn" onClick={() => void scanRewardsNow()}>
                      Scan now
                    </button>
                    <button
                      className={watcherEnabled ? "btn" : "btn btn-ghost"}
                      onClick={() => void toggleRewardWatcher()}
                    >
                      {watcherEnabled ? "Stop watcher" : "Start watcher"}
                    </button>
                    <button
                      className="btn btn-ghost"
                      onClick={() => void loadOverlayDiagnostics()}
                    >
                      <RefreshCw size={13} />
                      Refresh
                    </button>
                  </div>
                </div>
                <div className={styles.dataGrid}>
                  <span className="faint">Overlay</span>
                  <span className="num">{desktopCaps.overlayMode}</span>
                  <span className="faint">Session</span>
                  <span className="num">{desktopCaps.sessionKind}</span>
                  <span className="faint">Capture</span>
                  <span className="num">{desktopCaps.captureBackend}</span>
                  <span className="faint">Region picker</span>
                  <span className="num">{desktopCaps.regionPicker}</span>
                  <span className="faint">Plugin protocol</span>
                  <span className="num">
                    {desktopCaps.hyprOverlay?.protocolVersion ?? "—"}
                  </span>
                  <span className="faint">Watcher</span>
                  <span className="num">{watcherEnabled ? "active" : "off"}</span>
                  <span className="faint">Region</span>
                  <span className="num">
                    {captureRegion
                      ? `${captureRegion.x},${captureRegion.y} ${captureRegion.width}×${captureRegion.height}`
                      : "—"}
                  </span>
                  <span className="faint">Last scan</span>
                  <span className="num">
                    {scanDiagnostics?.updatedAt
                      ? new Date(scanDiagnostics.updatedAt).toLocaleTimeString()
                      : "—"}
                  </span>
                  <span className="faint">OCR crop</span>
                  <span className="num">
                    {scanDiagnostics?.selectedCrop !== undefined
                      ? `${Math.round(scanDiagnostics.selectedCrop * 100)}% · ${scanDiagnostics.selectedScale ?? "?"}×`
                      : "—"}
                  </span>
                  <span className="faint">OCR engine</span>
                  <span className="num">{scanDiagnostics?.ocrBackend ?? "—"}</span>
                  <span className="faint">Scan latency</span>
                  <span className="num">
                    {scanDiagnostics?.totalMs !== undefined
                      ? `${(scanDiagnostics.totalMs / 1000).toFixed(1)}s total · ${((scanDiagnostics.fastOcrMs ?? 0) / 1000).toFixed(1)}s fast`
                      : "—"}
                  </span>
                </div>
                {scanDiagnostics?.resolvedRows?.length ? (
                  <p className={`${styles.note} success`}>
                    Resolved: {scanDiagnostics.resolvedRows.join(" · ")}
                  </p>
                ) : null}
                {scanDiagnostics?.error && (
                  <p className={`${styles.note} danger`}>{scanDiagnostics.error}</p>
                )}
                {scanDiagnostics?.rawText && (
                  <details className={styles.note}>
                    <summary>Raw OCR text</summary>
                    <pre style={{ whiteSpace: "pre-wrap", margin: "8px 0 0" }}>
                      {scanDiagnostics.rawText}
                    </pre>
                  </details>
                )}
                {scanDiagnostics?.lineRows?.length ? (
                  <details className={styles.note}>
                    <summary>Row alignment</summary>
                    <pre style={{ whiteSpace: "pre-wrap", margin: "8px 0 0" }}>
                      {scanDiagnostics.lineRows.join("\n")}
                    </pre>
                  </details>
                ) : null}
              </div>
            )}
          </section>

          {/* ---- LEAGUE RULESET ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">League ruleset</span>
            </div>
            <div className="field-row">
              <label className="field-label">Engine rules</label>
              <div className="seg">
                <button
                  className={engineLeague === "challenge" ? "on" : ""}
                  onClick={() => void setEngineLeague("challenge")}
                  title="Runes of Aldur (0.5 challenge league)"
                >
                  Runes of Aldur
                </button>
                <button
                  className={engineLeague === "standard" ? "on" : ""}
                  onClick={() => void setEngineLeague("standard")}
                  title="Standard (legacy items + legacy currencies)"
                >
                  Standard
                </button>
              </div>
            </div>
            <p className={`${styles.note} faint`}>
              In 0.5 the Recombinator and the Corruption / Homogenising omens only
              work in Standard. The advisor drops illegal moves for the selected
              ruleset and re-plans on change.
            </p>
          </section>

          {/* ---- CAPTURE DAEMON ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Capture</span>
              <span
                className={`chip ${captureStatus === "connected" ? "success" : "faint"}`}
                title="poc2-capture daemon on ws://127.0.0.1:17771"
              >
                {captureStatus === "connected"
                  ? `connected${captureDaemonVersion ? ` · v${captureDaemonVersion}` : ""}`
                  : "daemon not running"}
              </span>
            </div>
            <p className={`${styles.note} muted`}>
              Hover an item in PoE2 and press <span className="num">CTRL+SHIFT+D</span> — the
              daemon presses the game&apos;s own Ctrl+C, reads the clipboard and imports the
              item here instantly. <span className="num">CTRL+SHIFT+A</span> for advanced mod
              tiers, <span className="num">CTRL+SHIFT+S</span> for screenshot-OCR.
            </p>
            <p className={`${styles.note} faint`}>
              Setup: <span className="num">cargo install --path crates/capture</span>, then
              source <span className="num">examples/hyprland/poc2-capture.conf</span> from your
              Hyprland config (see ADR-0011). The web app works fully without it.
            </p>
            {captureLastAt && (
              <p className={`${styles.note} success`}>
                Last capture: <span className="num">{new Date(captureLastAt).toLocaleTimeString()}</span>
              </p>
            )}
            {captureLastError && (
              <p className={`${styles.note} danger`}>{captureLastError}</p>
            )}
          </section>

          {/* ---- PLUGINS (ADR-0014 phase 1) ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Plugins</span>
              <label className="btn" title="Add a plugin .wasm built with poc2-plugin-sdk">
                <Puzzle size={13} /> Add plugin…
                <input
                  type="file"
                  accept=".wasm"
                  style={{ display: "none" }}
                  onChange={(e) => void onAddPlugin(e)}
                />
              </label>
            </div>
            {plugins.length === 0 && (
              <p className={`${styles.note} faint`}>
                No plugins loaded. Plugins built with{" "}
                <span className="num">poc2-plugin-sdk</span> can ship extra strategies,
                rules and custom predicates (see{" "}
                <span className="num">examples/plugins/</span>); they run sandboxed with
                no network or filesystem access. The file name is the plugin id that
                rule TOMLs reference. Recommendation emitters are future work (ADR-0014).
              </p>
            )}
            {plugins.map((p) => (
              <div key={p.name} className={styles.sectionHead}>
                <span>
                  <span className={p.error ? "danger" : undefined}>{p.name}</span>{" "}
                  <span className="faint num">
                    {p.error ??
                      `${p.strategies} strategies · ${p.rules} rules${p.predicates ? " · predicates" : ""}`}
                  </span>
                </span>
                <button
                  className="btn btn-ghost"
                  onClick={() => void onRemovePlugin(p.name)}
                  title="Remove this plugin"
                  aria-label={`Remove plugin ${p.name}`}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            ))}
          </section>

          {/* ---- NOTES ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Notes</span>
            </div>
            <textarea
              className={`field ${styles.notes}`}
              value={notes}
              placeholder="Free-form notes for this craft project…"
              onChange={(e) => setNotes(e.target.value)}
              rows={6}
              spellCheck={false}
            />
          </section>

          {/* ---- DATA ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Data</span>
            </div>
            <div className={styles.dataGrid}>
              <span className="faint">Patch</span>
              <span className="num">{patch || "—"}</span>
              <span className="faint">Modifiers</span>
              <span className="num">
                {modCount > 0 ? `${modCount.toLocaleString()} mods loaded` : "—"}
              </span>
            </div>
            <div className={styles.engineNote}>
              <Cpu size={13} className="faint" />
              <span className="muted">
                The crafting engine runs fully client-side in WebAssembly — no server,
                no telemetry.
              </span>
            </div>
          </section>

          {/* ---- RESET ---- */}
          <section className={`card ${styles.section}`}>
            <div className={styles.sectionHead}>
              <span className="eyebrow">Reset</span>
            </div>
            <div className={styles.resetRow}>
              <button
                className="btn"
                onClick={loadFixture}
                title="Load the worked-example item and goal"
              >
                <Database size={13} className="faint" />
                Load worked example
              </button>
              <button
                className={`btn ${styles.danger}`}
                onClick={confirmClear}
                title="Erase all recorded crafting steps"
              >
                <Trash2 size={13} />
                Clear history
              </button>
            </div>
            <p className={`${styles.note} faint`}>
              <RotateCcw size={11} className="faint" /> Loading the worked example
              resets the item, goal and history.
            </p>
          </section>
        </div>
      </div>
    </div>
  );
}
