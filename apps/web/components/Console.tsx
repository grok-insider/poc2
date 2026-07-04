"use client";

import {
  Box,
  Coins,
  Crosshair,
  Database,
  FlaskConical,
  History,
  Layers,
  RefreshCw,
  Regex,
  Settings,
  Sprout,
  Undo2,
  Wand2,
} from "lucide-react";
import { useCraft, type Section } from "@/lib/store";
import { ItemCard } from "@/components/ItemCard";
import { TargetSummary } from "@/components/TargetSummary";
import { GuidePanel } from "@/components/GuidePanel";
import { LedgerDock } from "@/components/LedgerDock";
import { ItemEditor } from "@/components/ItemEditor";
import { TargetEditor } from "@/components/TargetEditor";
import { EligibleTab } from "@/components/EligibleTab";
import { HistoryTab } from "@/components/HistoryTab";
import { DatabasePanel } from "@/components/DatabasePanel";
import { GenesisPanel } from "@/components/GenesisPanel";
import { PricePanel } from "@/components/PricePanel";
import { RegexPanel } from "@/components/RegexPanel";
import { ToolsPanel } from "@/components/ToolsPanel";
import { SettingsPanel } from "@/components/SettingsPanel";
import { OutcomeDialog } from "@/components/OutcomeDialog";
import { humanizeId } from "@/lib/format";
import type { LucideIcon } from "lucide-react";
import styles from "./Console.module.css";

const RAIL: { icon: LucideIcon; label: string; section: Section }[] = [
  { icon: Box, label: "Item", section: "item" },
  { icon: Crosshair, label: "Target", section: "target" },
  { icon: Wand2, label: "Guide", section: "guide" },
  { icon: Layers, label: "Eligible", section: "eligible" },
  { icon: History, label: "History", section: "history" },
  { icon: Database, label: "Database", section: "database" },
  { icon: Coins, label: "Price", section: "price" },
  { icon: Regex, label: "Regex", section: "regex" },
  { icon: Sprout, label: "Genesis Tree", section: "genesis" },
  { icon: FlaskConical, label: "Tools", section: "tools" },
  { icon: Settings, label: "Settings", section: "settings" },
];

function ActivePane({ section }: { section: Section }) {
  switch (section) {
    case "item":
      return <ItemEditor />;
    case "target":
      return <TargetEditor />;
    case "guide":
      return <GuidePanel />;
    case "eligible":
      return <EligibleTab />;
    case "history":
      return <HistoryTab />;
    case "database":
      return <DatabasePanel />;
    case "price":
      return <PricePanel />;
    case "regex":
      return <RegexPanel />;
    case "genesis":
      return <GenesisPanel />;
    case "tools":
      return <ToolsPanel />;
    case "settings":
      return <SettingsPanel />;
    default:
      return <GuidePanel />;
  }
}

export function Console() {
  const item = useCraft((s) => s.item);
  const patch = useCraft((s) => s.patch);
  const planning = useCraft((s) => s.planning);
  const replan = useCraft((s) => s.replan);
  const section = useCraft((s) => s.section);
  const setSection = useCraft((s) => s.setSection);
  const outcomeOpen = useCraft((s) => s.outcomeOpen);
  const historyLen = useCraft((s) => s.history.length);
  const undo = useCraft((s) => s.undo);
  const captureStatus = useCraft((s) => s.captureStatus);
  const trainedModels = useCraft((s) => s.trainedModels);

  return (
    <div className={styles.app}>
      <header className={`${styles.topbar} glass`}>
        <div className={styles.brand}>
          <span className={styles.mark}>⬡</span>
          <span className={styles.wordmark}>PATH OF CRAFTING</span>
        </div>
        <div className={styles.itemline}>
          <span className={`r-${item.rarity}`}>
            {item.base_display_name ?? humanizeId(item.base)}
          </span>
          <span className="faint"> · </span>
          <span className="muted">{item.rarity}</span>
          <span className="faint"> · </span>
          <span className="num muted">ilvl {item.ilvl}</span>
        </div>
        <div className={styles.topright}>
          {captureStatus === "connected" && (
            <span
              className="chip"
              title="poc2-capture daemon connected — CTRL+SHIFT+D captures the hovered item"
            >
              <span className={styles.captureDot} aria-hidden />
              capture
            </span>
          )}
          <button
            className="btn btn-ghost"
            onClick={undo}
            disabled={historyLen === 0}
            title="Undo last recorded outcome"
            aria-label="Undo"
          >
            <Undo2 size={15} />
          </button>
          {trainedModels > 0 && (
            <span
              className="chip num"
              title={`${trainedModels} trained (goal × class) Q-models held — solved on demand per goal (plus any preloaded artefact); the planner blends trained-policy scores with heuristics`}
            >
              ⚛ {trainedModels}
            </span>
          )}
          <span className="chip num">patch {patch}</span>
          <button
            className="btn btn-ghost"
            onClick={() => void replan()}
            title="Re-plan"
            aria-label="Re-plan"
          >
            <RefreshCw size={15} className={planning ? styles.spin : ""} />
          </button>
        </div>
      </header>

      <nav className={styles.rail} aria-label="Sections">
        {RAIL.map(({ icon: Icon, label, section: s }) => (
          <button
            key={label}
            className={`${styles.railBtn} ${section === s ? styles.railActive : ""}`}
            title={label}
            aria-label={label}
            aria-current={section === s ? "page" : undefined}
            onClick={() => setSection(s)}
          >
            <Icon size={19} strokeWidth={1.75} />
          </button>
        ))}
      </nav>

      <main
        className={`${styles.canvas} ${section === "genesis" ? styles.canvasFull : ""}`}
      >
        {section !== "genesis" && (
          <section className={styles.bench} aria-label="The bench — your item">
            <ItemCard />
            <TargetSummary />
          </section>
        )}
        <section className={styles.guide} aria-label="The guide">
          <ActivePane section={section} />
        </section>
      </main>

      <LedgerDock />

      {outcomeOpen && <OutcomeDialog />}
    </div>
  );
}
