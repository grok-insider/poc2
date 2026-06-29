"use client";

/// Genesis Tree panel (PoE2 0.5 "Return of the Ancients").
///
/// A 1:1-style recreation of the in-game "Brequel" Genesis Tree screen:
/// the five Womb branches rendered with the REAL in-game node frames
/// (BreachLeague PassiveFrame sprites referenced by BrequelTree.json),
/// Breach-style node tooltips, and a curated goal-preset sidebar whose
/// node sets are GRAPH-RESOLVED in the pipeline (steps + forced connector
/// nodes + honest point costs against the womb cap).
///
/// Layout: farming notes (left) | tree viewport (center, dominant) |
/// goal presets (right). UI-only knowledge — no birth simulation.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Check, Copy, ExternalLink, Maximize2, Sprout, XCircle } from "lucide-react";
import { engine } from "@/lib/engine/client";
import type {
  GenesisBranch,
  GenesisIconManifest,
  GenesisNode,
  GenesisPreset,
  GenesisTreeView,
} from "@/lib/types";
import styles from "./GenesisPanel.module.css";

const ICON_BASE = "/genesis-icons";
const UI = `${ICON_BASE}/ui`;

/** Womb display order (game order). */
const BRANCHES: GenesisBranch[] = ["currency", "ring", "amulet", "belt", "breachstone"];

const CONFIDENCE_LABEL: Record<string, string> = {
  measured: "measured sample",
  official: "official demo",
  community: "community estimate",
};

interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

function fitViewBox(nodes: GenesisNode[]): ViewBox {
  if (nodes.length === 0) return { x: -500, y: -500, w: 1000, h: 1000 };
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const n of nodes) {
    minX = Math.min(minX, n.x);
    minY = Math.min(minY, n.y);
    maxX = Math.max(maxX, n.x);
    maxY = Math.max(maxY, n.y);
  }
  const pad = 170;
  return { x: minX - pad, y: minY - pad, w: maxX - minX + pad * 2, h: maxY - minY + pad * 2 };
}

/** Per-node highlight role under the active preset. */
type Role =
  | { kind: "step"; priority: number; why: string; optional: boolean }
  | { kind: "fill"; why: string }
  | { kind: "connector" }
  | { kind: "avoid"; why: string };

export function GenesisPanel() {
  const [view, setView] = useState<GenesisTreeView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [branch, setBranch] = useState<GenesisBranch>("currency");
  const [presetId, setPresetId] = useState<string | null>(null);
  const [hover, setHover] = useState<{ node: GenesisNode; px: number; py: number } | null>(null);
  const [viewBoxOverride, setViewBoxOverride] = useState<ViewBox | null>(null);
  const [copied, setCopied] = useState(false);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const drag = useRef<{ x: number; y: number; vb: ViewBox } | null>(null);

  // ---- data load -----------------------------------------------------------
  useEffect(() => {
    let live = true;
    engine
      .genesisTree()
      .then((v) => live && setView(v))
      .catch((e: unknown) => live && setError(String(e)));
    fetch(`${ICON_BASE}/manifest.json`, { cache: "no-cache" })
      .then((r) => (r.ok ? r.json() : null))
      .then((m: GenesisIconManifest | null) => {
        if (live && m?.entries) setIcons(m.entries);
      })
      .catch(() => {
        /* icons are an optional regenerable artifact */
      });
    return () => {
      live = false;
    };
  }, []);

  // ---- derived -------------------------------------------------------------
  const womb = useMemo(() => view?.wombs.find((w) => w.branch === branch) ?? null, [view, branch]);
  const branchNodes = useMemo(
    () => (view?.nodes ?? []).filter((n) => n.branch === branch),
    [view, branch],
  );
  const nodeById = useMemo(() => {
    const m = new Map<string, GenesisNode>();
    for (const n of branchNodes) m.set(n.id, n);
    return m;
  }, [branchNodes]);

  const edges = useMemo(() => {
    const seen = new Set<string>();
    const out: { a: GenesisNode; b: GenesisNode }[] = [];
    for (const n of branchNodes) {
      for (const cid of n.connections) {
        const other = nodeById.get(cid);
        if (!other) continue;
        const key = n.id < cid ? `${n.id}|${cid}` : `${cid}|${n.id}`;
        if (!seen.has(key)) {
          seen.add(key);
          out.push({ a: n, b: other });
        }
      }
    }
    return out;
  }, [branchNodes, nodeById]);

  const presets = useMemo(
    () => (view?.presets ?? []).filter((p) => p.womb === branch),
    [view, branch],
  );
  const preset: GenesisPreset | null = useMemo(
    () => view?.presets.find((p) => p.id === presetId) ?? null,
    [view, presetId],
  );

  /** node id → highlight role for the active preset (graph-resolved ids). */
  const roleById = useMemo(() => {
    const m = new Map<string, Role>();
    if (!preset) return m;
    // Connectors first so steps/avoid can override their own nodes.
    for (const s of preset.steps) {
      for (const id of s.connector_ids) m.set(id, { kind: "connector" });
    }
    for (const s of preset.steps) {
      for (const id of s.node_ids) {
        m.set(
          id,
          s.fill
            ? { kind: "fill", why: s.why }
            : { kind: "step", priority: s.priority, why: s.why, optional: s.optional },
        );
      }
    }
    for (const a of preset.avoid) {
      for (const id of a.node_ids) m.set(id, { kind: "avoid", why: a.why });
    }
    return m;
  }, [preset]);

  const presetActive = preset !== null;

  // ---- viewport ------------------------------------------------------------
  const fitted = useMemo(() => fitViewBox(branchNodes), [branchNodes]);
  const viewBox = viewBoxOverride ?? fitted;
  const setViewBox = setViewBoxOverride;
  const refit = useCallback(() => setViewBoxOverride(null), []);

  const selectBranch = (b: GenesisBranch) => {
    setBranch(b);
    setPresetId(null);
    setHover(null);
    setViewBoxOverride(null);
  };

  const selectPreset = (p: GenesisPreset) => {
    if (presetId === p.id) {
      setPresetId(null);
      return;
    }
    setPresetId(p.id);
    if (p.womb === "breachstone" && p.steps.length > 0) setBranch("currency");
    else if (p.womb !== branch) setBranch(p.womb);
  };

  const onWheel = (e: React.WheelEvent<SVGSVGElement>) => {
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const mx = viewBox.x + ((e.clientX - rect.left) / rect.width) * viewBox.w;
    const my = viewBox.y + ((e.clientY - rect.top) / rect.height) * viewBox.h;
    const factor = e.deltaY > 0 ? 1.18 : 1 / 1.18;
    const w = Math.min(Math.max(viewBox.w * factor, 400), 16000);
    const h = (w / viewBox.w) * viewBox.h;
    setViewBox({
      x: mx - ((mx - viewBox.x) / viewBox.w) * w,
      y: my - ((my - viewBox.y) / viewBox.h) * h,
      w,
      h,
    });
  };

  const onPointerDown = (e: React.PointerEvent<SVGSVGElement>) => {
    drag.current = { x: e.clientX, y: e.clientY, vb: viewBox };
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (!drag.current) return;
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const dx = ((e.clientX - drag.current.x) / rect.width) * drag.current.vb.w;
    const dy = ((e.clientY - drag.current.y) / rect.height) * drag.current.vb.h;
    setViewBox({
      ...drag.current.vb,
      x: drag.current.vb.x - dx,
      y: drag.current.vb.y - dy,
    });
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const onNodeHover = (n: GenesisNode, e: React.PointerEvent) => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const rect = wrap.getBoundingClientRect();
    const px = Math.min(e.clientX - rect.left + 18, rect.width - 320);
    setHover({ node: n, px, py: Math.min(e.clientY - rect.top + 14, rect.height - 180) });
  };

  const copyNodeList = () => {
    if (!preset) return;
    const lines = preset.steps
      .slice()
      .sort((a, b) => a.priority - b.priority)
      .map((s) => {
        const tag = s.optional ? " (respec option)" : s.fill ? " (fill remaining points)" : "";
        return `${s.priority}. ${s.node}${tag} — ${s.why}`;
      });
    if (preset.avoid.length > 0) {
      lines.push("", "Avoid:");
      for (const a of preset.avoid) lines.push(`- ${a.node} — ${a.why}`);
    }
    lines.push("", `Core path: ${preset.core_points}/${preset.points_cap} points (connectors included).`);
    void navigator.clipboard.writeText(`${preset.name} (${preset.womb} womb)\n${lines.join("\n")}`);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1400);
  };

  // ---- render helpers --------------------------------------------------------
  const iconHref = (key: string): string | null => {
    const file = icons[key];
    return file ? `${ICON_BASE}/${file}` : null;
  };

  /** In-game frame sprite for a node under the current preset role. */
  const frameHref = (n: GenesisNode, role: Role | undefined): string => {
    if (n.womb_slot) {
      return role || !presetActive ? `${UI}/frame-womb-slot-active.webp` : `${UI}/frame-womb-slot.webp`;
    }
    const kind = n.notable ? "notable" : "small";
    if (!presetActive) return `${UI}/frame-${kind}-normal.webp`;
    if (!role) return `${UI}/frame-${kind}-normal.webp`;
    switch (role.kind) {
      case "step":
        return role.optional
          ? `${UI}/frame-${kind}-canallocate.webp`
          : `${UI}/frame-${kind}-active.webp`;
      case "fill":
        return `${UI}/frame-${kind}-canallocate.webp`;
      case "connector":
        return `${UI}/frame-${kind}-active.webp`;
      case "avoid":
        return `${UI}/frame-${kind}-normal.webp`;
    }
  };

  const renderNode = (n: GenesisNode) => {
    const isStart = n.start || n.name === "";
    const role = roleById.get(n.id);
    const dimmed = presetActive && !role && !n.womb_slot && !isStart;
    const r = n.womb_slot ? 62 : n.notable ? 46 : 30;
    const frameR = n.womb_slot ? r * 1.12 : n.notable ? r * 1.42 : r * 1.6;
    const icon = iconHref(n.womb_slot && womb ? womb.gift_art : n.icon);
    const showGlow =
      role && (role.kind === "step" || role.kind === "connector" || role.kind === "fill");

    if (isStart && !n.womb_slot) {
      return (
        <g key={n.id} transform={`translate(${n.x}, ${n.y})`} className={styles.node}>
          <circle r={10} className={styles.startDot} />
        </g>
      );
    }

    return (
      <g
        key={n.id}
        className={`${styles.node} ${dimmed ? styles.nodeDim : ""}`}
        transform={`translate(${n.x}, ${n.y})`}
        onPointerEnter={(e) => onNodeHover(n, e)}
        onPointerMove={(e) => onNodeHover(n, e)}
        onPointerLeave={() => setHover(null)}
      >
        {showGlow && (
          <image
            href={`${UI}/node-glow.webp`}
            x={-frameR * 1.25}
            y={-frameR * 1.25}
            width={frameR * 2.5}
            height={frameR * 2.5}
            className={styles.nodeGlow}
            style={{ pointerEvents: "none" }}
          />
        )}
        {/* node icon under the frame ring */}
        {icon && (
          <image
            href={icon}
            x={-r}
            y={-r}
            width={r * 2}
            height={r * 2}
            preserveAspectRatio="xMidYMid meet"
            style={{ pointerEvents: "none" }}
          />
        )}
        {/* the REAL in-game frame sprite */}
        <image
          href={frameHref(n, role)}
          x={-frameR}
          y={-frameR}
          width={frameR * 2}
          height={frameR * 2}
          preserveAspectRatio="xMidYMid meet"
          style={{ pointerEvents: "none" }}
        />
        {/* hover/hit circle */}
        <circle r={frameR * 0.8} fill="transparent" />
        {/* preset adornments */}
        {role?.kind === "step" && (
          <g transform={`translate(${r * 1.05}, ${-r * 1.05})`}>
            <circle r={16} className={role.optional ? styles.stepBadgeOpt : styles.stepBadge} />
            <text y={6} textAnchor="middle" className={styles.stepBadgeText}>
              {role.priority}
            </text>
          </g>
        )}
        {role?.kind === "avoid" && (
          <>
            <circle r={frameR * 0.78} className={styles.avoidHalo} />
            <text y={-frameR - 8} textAnchor="middle" className={styles.avoidMark}>
              ✕
            </text>
          </>
        )}
      </g>
    );
  };

  // ---- tooltip body ---------------------------------------------------------
  const hoverRole = hover ? roleById.get(hover.node.id) : undefined;

  // ---- empty / error states ---------------------------------------------------
  if (error) {
    return (
      <div className="pane">
        <div className="pane-head">
          <div className="pane-title">The Genesis Tree</div>
        </div>
        <div className="empty-state">
          <span className="eyebrow danger">Genesis Tree error</span>
          <span className="faint mono" style={{ fontSize: 11 }}>
            {error}
          </span>
        </div>
      </div>
    );
  }

  if (view && !view.available) {
    return (
      <div className="pane">
        <div className="pane-head">
          <div className="pane-title">The Genesis Tree</div>
        </div>
        <div className="empty-state">
          <Sprout size={20} className="faint" />
          <span className="muted">
            This bundle has no Genesis Tree data — it ships with 0.5+ bundles (Return of the
            Ancients).
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="pane">
      <div className={`pane-head ${styles.head}`}>
        <div className="poe-plaque">The Genesis Tree</div>
        <div className={styles.headActions}>
          <div className="seg">
            {BRANCHES.map((b) => {
              const w = view?.wombs.find((x) => x.branch === b);
              return (
                <button
                  key={b}
                  className={branch === b ? "on" : ""}
                  onClick={() => selectBranch(b)}
                  title={w ? `${w.display_name} — ${w.wombgift}` : b}
                >
                  {w?.display_name.replace(" Womb", "") ?? b}
                  {w && w.points > 0 && <span className={styles.pointCap}>{w.points}</span>}
                </button>
              );
            })}
          </div>
          <button
            className="btn btn-ghost"
            onClick={refit}
            title="Fit tree to view"
            aria-label="Fit tree to view"
          >
            <Maximize2 size={14} />
          </button>
        </div>
      </div>

      <div className={styles.layout}>
        {/* ---- farming notes (left) ---- */}
        <aside className={styles.farmCol}>
          <div className="poe-section">Hiveblood &amp; Wombgifts</div>
          {womb && (
            <div className={`poe-pop poe-pop--currency ${styles.giftCard}`}>
              <div className="poe-pop-header">
                <span>{womb.wombgift}</span>
              </div>
              <div className="poe-pop-content">
                {iconHref(womb.gift_art) && (
                  // eslint-disable-next-line @next/next/no-img-element
                  <img
                    src={iconHref(womb.gift_art) ?? undefined}
                    alt={womb.wombgift}
                    width={44}
                    height={44}
                    className={styles.giftArt}
                  />
                )}
                {womb.points > 0 && (
                  <div>
                    Womb passives:{" "}
                    <span className={styles.valueText}>{womb.points} points</span>
                  </div>
                )}
                <div className="poe-pop-sep" />
                <div className="poe-pop-note">{womb.blurb}</div>
              </div>
            </div>
          )}
          {view && view.farming_notes.length > 0 && (
            <ul className={styles.farmNotes}>
              {view.farming_notes.map((n) => (
                <li key={n.slice(0, 32)}>{n}</li>
              ))}
            </ul>
          )}
          {view && view.videos.length > 0 && (
            <>
              <div className="poe-section">Vetted guides</div>
              <div className={styles.videos}>
                {view.videos.map((v) => (
                  <a
                    key={v.url}
                    href={v.url}
                    target="_blank"
                    rel="noreferrer noopener"
                    className={styles.sourceLink}
                  >
                    <ExternalLink size={11} />
                    <span>
                      {v.channel} — {v.title}
                    </span>
                  </a>
                ))}
              </div>
            </>
          )}
          <div className={styles.disclaimer}>
            Numbers are community estimates from early 0.5 — verify against your league economy.
          </div>
        </aside>

        {/* ---- tree viewport (center) ---- */}
        <div className={styles.treeWrap} ref={wrapRef}>
          {!view ? (
            <div className="skeleton" style={{ height: "100%" }} />
          ) : branchNodes.length === 0 ? (
            <div className={styles.noNodes}>
              <Sprout size={20} className="faint" />
              <span className="muted">{womb?.blurb ?? "This womb has no allocatable passives."}</span>
            </div>
          ) : (
            <svg
              ref={svgRef}
              className={styles.tree}
              viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`}
              onWheel={onWheel}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
              role="img"
              aria-label={`${womb?.display_name ?? branch} passive tree`}
            >
              <g>
                {edges.map(({ a, b }) => {
                  const lit =
                    presetActive &&
                    roleById.has(a.id) &&
                    roleById.has(b.id) &&
                    roleById.get(a.id)?.kind !== "avoid" &&
                    roleById.get(b.id)?.kind !== "avoid";
                  return (
                    <line
                      key={`${a.id}|${b.id}`}
                      x1={a.x}
                      y1={a.y}
                      x2={b.x}
                      y2={b.y}
                      className={lit ? styles.edgeLit : styles.edge}
                    />
                  );
                })}
              </g>
              <g>{branchNodes.map(renderNode)}</g>
            </svg>
          )}

          {/* Breach-style node tooltip (in-game reference) */}
          {hover && (
            <div className={styles.gtip} style={{ left: hover.px, top: hover.py }}>
              <div className={`${styles.gtipHead} ${hover.node.notable ? styles.gtipHeadNotable : ""}`}>
                <span>
                  {hover.node.womb_slot ? womb?.display_name : hover.node.name || "Womb Root"}
                </span>
              </div>
              <div className={styles.gtipBody}>
                {hover.node.womb_slot ? (
                  <div className={styles.gtipItalic}>
                    Place a {womb?.wombgift ?? "Wombgift"} in this Womb
                  </div>
                ) : hover.node.start ? (
                  <div className={styles.gtipItalic}>
                    Branch entry — connected passives unlock as you Birth items with this womb.
                  </div>
                ) : (
                  <div className={styles.gtipMods}>
                    {hover.node.description.split("\n").map((l) => (
                      <div key={l}>{l}</div>
                    ))}
                  </div>
                )}
                {hoverRole?.kind === "step" && (
                  <div className={styles.gtipAction}>
                    {hoverRole.optional ? "Respec option" : `Step ${hoverRole.priority}`} —{" "}
                    {hoverRole.why}
                  </div>
                )}
                {hoverRole?.kind === "fill" && (
                  <div className={styles.gtipAction}>Fill remaining points — {hoverRole.why}</div>
                )}
                {hoverRole?.kind === "connector" && (
                  <div className={styles.gtipItalic}>Pathing node — allocate to connect the route</div>
                )}
                {hoverRole?.kind === "avoid" && (
                  <div className={styles.gtipAvoid}>{hoverRole.why}</div>
                )}
              </div>
            </div>
          )}
        </div>

        {/* ---- preset sidebar (right) ---- */}
        <aside className={styles.side}>
          <div className="poe-section">Goal presets</div>
          {presets.length === 0 ? (
            <div className={styles.sideEmpty}>No curated presets for this womb yet.</div>
          ) : (
            presets.map((p) => (
              <button
                key={p.id}
                className={`${styles.preset} ${presetId === p.id ? styles.presetActive : ""}`}
                onClick={() => selectPreset(p)}
              >
                <div className={styles.presetTop}>
                  <span className={styles.presetName}>{p.name}</span>
                  <span className={`${styles.conf} ${styles[`conf_${p.confidence}`] ?? ""}`}>
                    {CONFIDENCE_LABEL[p.confidence] ?? p.confidence}
                  </span>
                </div>
                <div className={styles.presetSummary}>{p.summary}</div>
              </button>
            ))
          )}

          {preset && (
            <div className={`card ${styles.detail}`}>
              <div className={styles.detailHead}>
                <span>{preset.name}</span>
                <span className={styles.budget}>
                  <span className={preset.core_points > preset.points_cap ? "danger" : "gold"}>
                    {preset.core_points}
                  </span>
                  <span className="faint">/{preset.points_cap} pts</span>
                </span>
                <button
                  className="btn btn-ghost"
                  onClick={copyNodeList}
                  title="Copy node list"
                  aria-label="Copy node list"
                >
                  {copied ? <Check size={14} /> : <Copy size={14} />}
                </button>
              </div>
              <ol className={styles.steps}>
                {preset.steps
                  .slice()
                  .sort((a, b) => a.priority - b.priority)
                  .map((s) => (
                    <li
                      key={`${s.priority}-${s.node}`}
                      className={s.optional ? styles.stepOptional : ""}
                    >
                      <span className={styles.stepNode}>
                        {s.node}
                        {s.fill && <span className={styles.stepTag}>fill</span>}
                        {s.optional && <span className={styles.stepTag}>respec</span>}
                        {!s.fill && !s.optional && (
                          <span className={styles.stepPts}>{s.points_after} pts</span>
                        )}
                      </span>
                      <span className={styles.stepWhy}>{s.why}</span>
                      {s.connector_ids.length > 0 && (
                        <span className={styles.stepConn}>
                          +{s.connector_ids.length} pathing node
                          {s.connector_ids.length > 1 ? "s" : ""}
                        </span>
                      )}
                    </li>
                  ))}
              </ol>
              {preset.avoid.length > 0 && (
                <div className={styles.avoidBlock}>
                  {preset.avoid.map((a) => (
                    <div key={a.node} className={styles.avoidRow}>
                      <XCircle size={12} />
                      <span>
                        <strong>{a.node}</strong> — {a.why}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {preset.gift_advice && <div className={styles.giftAdvice}>{preset.gift_advice}</div>}
              {preset.sources.length > 0 && (
                <div className={styles.sources}>
                  {preset.sources.map((s) => {
                    const urlStart = s.lastIndexOf("http");
                    const label = urlStart > 0 ? s.slice(0, urlStart).replace(/[—-]\s*$/, "") : s;
                    const url = urlStart >= 0 ? s.slice(urlStart) : null;
                    return url ? (
                      <a
                        key={s}
                        href={url}
                        target="_blank"
                        rel="noreferrer noopener"
                        className={styles.sourceLink}
                      >
                        <ExternalLink size={11} />
                        <span>{label.trim()}</span>
                      </a>
                    ) : (
                      <span key={s}>{s}</span>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
