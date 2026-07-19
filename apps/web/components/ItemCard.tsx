"use client";

import { useCraft } from "@/lib/store";
import { attributeVariant } from "@/lib/concepts";
import { affixCounts, humanizeId, humanizeModId, modValue, rarityClass } from "@/lib/format";
import type { ModRoll } from "@/lib/types";
import { BaseIcon } from "@/components/BaseIcon";
import styles from "./ItemCard.module.css";

function Diamonds({ used, max }: { used: number; max: number }) {
  return (
    <span className={styles.diamonds} aria-label={`${used} of ${max} used`}>
      {Array.from({ length: max }, (_, i) => (
        <span key={i} className={i < used ? styles.filled : styles.hollow}>
          {i < used ? "◈" : "◇"}
        </span>
      ))}
    </span>
  );
}

function ModRow({ m }: { m: ModRoll }) {
  const flags = [
    m.is_fractured ? "●" : null,
    m.kind === "desecrated" ? "✦" : null,
    m.kind === "corrupted" ? "☠" : null,
  ].filter(Boolean);
  return (
    <div className={styles.modRow}>
      <span className={styles.modVal + " num"}>{modValue(m) || "—"}</span>
      <span className={styles.modName}>{humanizeModId(m.mod_id)}</span>
      {flags.length > 0 && (
        <span className={styles.flags} title="mod flags">
          {flags.join(" ")}
        </span>
      )}
    </div>
  );
}

export function ItemCard() {
  const item = useCraft((s) => s.item);
  const eligible = useCraft((s) => s.eligible);
  const lastParse = useCraft((s) => s.lastParse);
  const counts = affixCounts(item);

  const variant = attributeVariant(eligible);
  const className = eligible?.item_class ?? lastParse?.itemClassId ?? null;
  const approxPool =
    (lastParse && !lastParse.baseResolved) || eligible?.data_available === false;

  return (
    <div className={`card ${styles.card}`}>
      <div className={`${styles.head} ${rarityClass(item.rarity)}`}>
        <BaseIcon
          baseId={item.base_type_id ?? item.base}
          name={item.base_display_name ?? humanizeId(item.base)}
          rarity={item.rarity}
          size={44}
        />
        <div>
          <div className={styles.name}>
            {item.base_display_name ?? humanizeId(item.base)}
          </div>
          <div className={styles.sub}>
            <span className="muted" style={{ textTransform: "capitalize" }}>
              {item.rarity}
            </span>
            <span className="faint"> · </span>
            <span className="num muted">ilvl {item.ilvl}</span>
            {className && (
              <>
                <span className="faint"> · </span>
                <span className="muted">{humanizeId(className)}</span>
              </>
            )}
            {variant && <span className="tag" style={{ marginLeft: 6 }}>{variant}</span>}
            {approxPool && (
              <span
                className="tag danger"
                style={{ marginLeft: 6 }}
                title="The base wasn't recognised, so the modifier pool shown is approximate."
              >
                approx. pool
              </span>
            )}
            {item.corrupted && <span className="r-corrupted"> · Corrupted</span>}
          </div>
        </div>
      </div>

      <div className={styles.slots}>
        <span className="faint">Prefix</span>
        <Diamonds used={counts.prefix.used} max={counts.prefix.max} />
        <span className="faint" style={{ marginLeft: 12 }}>
          Suffix
        </span>
        <Diamonds used={counts.suffix.used} max={counts.suffix.max} />
      </div>

      {item.prefixes.length === 0 && item.suffixes.length === 0 ? (
        <div className={styles.empty}>No explicit modifiers yet.</div>
      ) : (
        <div className={styles.mods}>
          {item.prefixes.length > 0 && (
            <>
              <div className={styles.group}>prefixes</div>
              {item.prefixes.map((m, i) => (
                <ModRow key={`p${i}`} m={m} />
              ))}
            </>
          )}
          {item.suffixes.length > 0 && (
            <>
              <div className={styles.group}>suffixes</div>
              {item.suffixes.map((m, i) => (
                <ModRow key={`s${i}`} m={m} />
              ))}
            </>
          )}
        </div>
      )}
    </div>
  );
}
