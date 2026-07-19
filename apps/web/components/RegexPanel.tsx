"use client";

/// Regex panel — generates strings for PoE2's in-game search boxes
/// (stash / vendor / reveal search: quoted AND terms, `|` OR, `!` NOT,
/// 250-char limit). Three generators:
///
///   Goal    — the current craft target as a "the item is done" matcher
///   Mods    — free selection from the base's real mod pool
///   Vendor  — shopping filters (class / rarity / ilvl / common mods)
///
/// Fragments are computed at runtime against the live bundle pool
/// (lib/regex/*) so they can never drift from the data. Inspired by the
/// community tool poe2.re (clean-room reimplementation — no code shared).

import { useRegex, type RegexTab } from "@/lib/regex/state";
import { RegexGoalTab } from "./RegexGoalTab";
import { RegexModsTab, RegexTabletTab, RegexWaystoneTab } from "./RegexModsTab";
import { RegexVendorTab } from "./RegexVendorTab";
import styles from "./RegexPanel.module.css";

const TABS: { id: RegexTab; label: string; title: string }[] = [
  { id: "goal", label: "Goal", title: "Search string from your craft target" },
  { id: "mods", label: "Item mods", title: "Pick mods from this base's pool" },
  { id: "waystone", label: "Waystone", title: "Waystone map-mod search strings" },
  { id: "tablet", label: "Tablet", title: "Precursor Tablet search strings" },
  { id: "vendor", label: "Vendor", title: "Shopping filters for vendor/stash search" },
];

export function RegexPanel() {
  const tab = useRegex((s) => s.tab);
  const setTab = useRegex((s) => s.setTab);

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Regex</div>
        <div className="seg">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={tab === t.id ? "on" : ""}
              onClick={() => setTab(t.id)}
              title={t.title}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      <div className="pane-scroll">
        <div className={styles.stack}>
          {tab === "goal" && <RegexGoalTab />}
          {tab === "mods" && <RegexModsTab />}
          {tab === "waystone" && <RegexWaystoneTab />}
          {tab === "tablet" && <RegexTabletTab />}
          {tab === "vendor" && <RegexVendorTab />}
        </div>
      </div>
    </div>
  );
}
