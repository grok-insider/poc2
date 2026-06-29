"use client";

import { useCallback, useEffect, useState } from "react";
import { Dice5, FlaskConical, Play, Trash2, Upload } from "lucide-react";
import { useCraft } from "@/lib/store";
import { engine } from "@/lib/engine/client";
import { actionLabel, div } from "@/lib/format";
import { deleteRecipe, listRecipes, saveRecipe } from "@/lib/persist";
import type { AdvisorAction, Recipe, TrialDistribution } from "@/lib/types";
import styles from "./ToolsPanel.module.css";

const TRIAL_PRESETS = [100, 500, 2000] as const;

/* ---------- (A) Simulation runner -------------------------------------- */

function Histogram({ hist }: { hist: Record<number, number> }) {
  const entries = Object.entries(hist)
    .map(([bucket, count]) => ({ bucket: Number(bucket), count }))
    .sort((a, b) => a.bucket - b.bucket);

  if (entries.length === 0) {
    return <div className="faint">No distribution data.</div>;
  }
  const max = Math.max(...entries.map((e) => e.count), 1);

  return (
    <div className={styles.chart} role="img" aria-label="Change-count distribution">
      {entries.map((e) => (
        <div key={e.bucket} className={styles.barCol}>
          <div
            className={styles.bar}
            style={{ height: `${(e.count / max) * 100}%` }}
            title={`${e.count} trial${e.count === 1 ? "" : "s"} · ${e.bucket} change${
              e.bucket === 1 ? "" : "s"
            }`}
          />
          <span className={`${styles.barLabel} num faint`}>{e.bucket}</span>
        </div>
      ))}
    </div>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "gold" | "success" }) {
  return (
    <div className={styles.stat}>
      <span className="eyebrow">{label}</span>
      <span className={`${styles.statValue} num ${tone ?? ""}`}>{value}</span>
    </div>
  );
}

function SimulationRunner() {
  const item = useCraft((s) => s.item);
  const recs = useCraft((s) => s.recommendations);

  const [selected, setSelected] = useState(0);
  const [trials, setTrials] = useState<number>(500);
  const [seed, setSeed] = useState<number>(0);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<TrialDistribution | null>(null);

  // Keep the selection valid as recommendations re-plan.
  const safeIndex = selected < recs.length ? selected : 0;
  const action: AdvisorAction | undefined = recs[safeIndex]?.action;

  function run() {
    if (!action) return;
    setRunning(true);
    setError(null);
    let safeSeed = 0n;
    try {
      safeSeed = BigInt(Math.max(0, Math.trunc(seed)) || 0);
    } catch {
      safeSeed = 0n;
    }
    engine
      .runNTrials(item, action, Math.max(1, Math.trunc(trials)), safeSeed)
      .then((dist) => {
        setResult(dist);
        setRunning(false);
      })
      .catch((e: unknown) => {
        setError(String(e));
        setRunning(false);
      });
  }

  const stderrPct = result ? (result.success_rate_stderr * 100).toFixed(1) : "";

  return (
    <div className={`card ${styles.section}`}>
      <div className={styles.sectionHead}>
        <FlaskConical size={14} className="faint" />
        <span className="section-title">Simulation runner</span>
      </div>

      <div className={styles.field}>
        <span className="field-label">Action</span>
        {recs.length === 0 ? (
          <span className="faint">No recommendation to simulate — set a goal first.</span>
        ) : (
          <select
            className="field"
            value={safeIndex}
            onChange={(e) => setSelected(Number(e.target.value))}
          >
            {recs.map((r, i) => (
              <option key={i} value={i}>
                {actionLabel(r.action)}
              </option>
            ))}
          </select>
        )}
      </div>

      <div className={styles.controls}>
        <div className={styles.field}>
          <span className="field-label">Trials</span>
          <div className={styles.trialRow}>
            <div className="seg">
              {TRIAL_PRESETS.map((n) => (
                <button
                  key={n}
                  className={trials === n ? "on" : ""}
                  onClick={() => setTrials(n)}
                  type="button"
                >
                  {n}
                </button>
              ))}
            </div>
            <input
              className={`field ${styles.numInput} num`}
              type="number"
              min={1}
              step={50}
              value={trials}
              onChange={(e) => setTrials(Number(e.target.value))}
              aria-label="Trial count"
            />
          </div>
        </div>
        <div className={`${styles.field} ${styles.seedField}`}>
          <span className="field-label">Seed</span>
          <input
            className={`field ${styles.numInput} num`}
            type="number"
            min={0}
            step={1}
            value={seed}
            onChange={(e) => setSeed(Number(e.target.value))}
            aria-label="RNG seed"
          />
        </div>
      </div>

      <button
        className="btn btn-primary"
        onClick={run}
        disabled={!action || running}
        type="button"
      >
        <Play size={14} />
        {running ? "Simulating…" : "Run simulation"}
      </button>

      {error && (
        <pre className={`mono danger ${styles.error}`}>{error}</pre>
      )}

      {running && !result && (
        <div className={styles.results}>
          <div className="skeleton" style={{ height: 44, width: "100%" }} />
          <div className="skeleton" style={{ height: 110, width: "100%", marginTop: 10 }} />
        </div>
      )}

      {result && !running && (
        <div className={styles.results}>
          <div className={styles.headline}>
            <span className={`${styles.bigPct} num success`}>
              {(result.success_rate * 100).toFixed(1)}%
            </span>
            <span className="faint num">± {stderrPct}%</span>
            <span className="faint">success over</span>
            <span className="num">{result.n_trials}</span>
            <span className="faint">trials</span>
          </div>

          <div className={styles.statGrid}>
            <Stat
              label="Mean changes"
              value={result.mean_change_count.toFixed(2)}
            />
            <Stat
              label="Cost / trial"
              value={div(result.cost_per_trial_div)}
              tone="gold"
            />
            <Stat
              label="Expected total"
              value={div(result.total_cost_div_expected)}
              tone="gold"
            />
          </div>

          <div className={styles.chartWrap}>
            <span className="eyebrow">Change-count distribution</span>
            <Histogram hist={result.change_count_histogram} />
          </div>
        </div>
      )}
    </div>
  );
}

/* ---------- (B) Recipe library ----------------------------------------- */

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function RecipeLibrary() {
  const item = useCraft((s) => s.item);
  const goal = useCraft((s) => s.goal);
  const setItem = useCraft((s) => s.setItem);
  const setGoal = useCraft((s) => s.setGoal);

  const [recipes, setRecipes] = useState<Recipe[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    let live = true;
    setLoading(true);
    listRecipes()
      .then((r) => {
        if (live) {
          setRecipes(r);
          setLoading(false);
        }
      })
      .catch(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => refresh(), [refresh]);

  async function saveCurrent() {
    const name = window.prompt("Recipe name")?.trim();
    if (!name) return;
    const description = window.prompt("Description (optional)")?.trim() ?? "";
    const recipe: Recipe = {
      name,
      description,
      item_json: JSON.stringify(item),
      goal_json: JSON.stringify(goal),
      created_at: new Date().toISOString(),
    };
    await saveRecipe(recipe);
    refresh();
  }

  function load(r: Recipe) {
    try {
      setItem(JSON.parse(r.item_json));
      setGoal(JSON.parse(r.goal_json));
    } catch {
      window.alert(`Recipe "${r.name}" is corrupted and could not be loaded.`);
    }
  }

  async function remove(r: Recipe) {
    if (!window.confirm(`Delete recipe "${r.name}"?`)) return;
    await deleteRecipe(r.name);
    refresh();
  }

  return (
    <div className={`card ${styles.section}`}>
      <div className={styles.sectionHead}>
        <Dice5 size={14} className="faint" />
        <span className="section-title">Recipe library</span>
        <button
          className="btn btn-ghost"
          onClick={() => void saveCurrent()}
          style={{ marginLeft: "auto" }}
          type="button"
        >
          <Upload size={13} />
          Save current
        </button>
      </div>

      {loading ? (
        <div className={styles.recipeList}>
          <div className="skeleton" style={{ height: 48, width: "100%" }} />
          <div className="skeleton" style={{ height: 48, width: "100%" }} />
        </div>
      ) : recipes.length === 0 ? (
        <div className="empty-state">
          <span className="muted">No saved recipes yet.</span>
          <span className="faint">Save the current item + goal to reuse it later.</span>
        </div>
      ) : (
        <div className={styles.recipeList}>
          {recipes.map((r) => (
            <div key={r.name} className={styles.recipe}>
              <div className={styles.recipeInfo}>
                <span className={styles.recipeName}>{r.name}</span>
                {r.description && (
                  <span className={`muted ${styles.recipeDesc}`}>{r.description}</span>
                )}
                <span className="faint num">{fmtDate(r.created_at)}</span>
              </div>
              <div className={styles.recipeActions}>
                <button className="btn" onClick={() => load(r)} type="button">
                  Load
                </button>
                <button
                  className="btn btn-ghost"
                  onClick={() => void remove(r)}
                  aria-label={`Delete ${r.name}`}
                  type="button"
                >
                  <Trash2 size={14} className="danger" />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ---------- Panel shell ------------------------------------------------- */

export function ToolsPanel() {
  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Tools</div>
      </div>
      <div className="pane-scroll">
        <div className={styles.stack}>
          <SimulationRunner />
          <RecipeLibrary />
        </div>
      </div>
    </div>
  );
}
