"use client";

/**
 * poe2db / in-game style item tooltip — core presentation for GUI + overlay.
 */

import { useEffect, useState, type ReactNode } from "react";
import {
  poePopKindClass,
  type ItemPopupModel,
  type ItemPopupModLine,
} from "@/lib/itemPopup";
import styles from "./ItemPopup.module.css";

const SIDE_LABEL: Record<string, string> = {
  implicit: "IMPLICIT",
  prefix: "PREFIX",
  suffix: "SUFFIX",
  unique: "UNIQUE",
  corruption: "CORRUPTION",
  rune: "RUNE",
  enchant: "ENCHANT",
  crafted: "CRAFTED",
  desecrated: "DESECRATED",
};

function ModLineView({
  line,
  odd,
  showSide,
}: {
  line: ItemPopupModLine;
  odd: boolean;
  showSide: boolean;
}) {
  const body = (() => {
    if (!line.values?.length) return line.text;
    const parts: Array<{ t: string; value: boolean }> = [];
    let rest = line.text;
    for (const v of line.values) {
      const idx = rest.indexOf(v);
      if (idx === -1) continue;
      if (idx > 0) parts.push({ t: rest.slice(0, idx), value: false });
      parts.push({ t: v, value: true });
      rest = rest.slice(idx + v.length);
    }
    if (rest) parts.push({ t: rest, value: false });
    if (parts.length === 0) return line.text;
    return parts.map((p, i) =>
      p.value ? (
        <span key={i} className="poe-pop-mod-value">
          {p.t}
        </span>
      ) : (
        <span key={i}>{p.t}</span>
      ),
    );
  })();

  if (!showSide && !line.tierLabel) {
    return (
      <div className={`poe-pop-mod${odd ? " poe-pop-mod--odd" : ""}`}>{body}</div>
    );
  }

  return (
    <div
      className={`poe-pop-mod-row${odd ? " poe-pop-mod--odd" : ""}${line.side === "corruption" ? " poe-pop-mod-row--corruption" : ""}`}
    >
      <span className="poe-pop-mod-side">
        {line.side ? SIDE_LABEL[line.side] ?? "" : ""}
      </span>
      <span className="poe-pop-mod poe-pop-mod-body">{body}</span>
      <span className="poe-pop-mod-tier">{line.tierLabel ?? ""}</span>
    </div>
  );
}

export function ItemPopup({
  model,
  artUrl,
  className,
}: {
  model: ItemPopupModel;
  artUrl?: string | null;
  className?: string;
}) {
  const [artFailed, setArtFailed] = useState(false);
  useEffect(() => {
    setArtFailed(false);
  }, [artUrl]);
  const showArt = Boolean(artUrl) && !artFailed;

  const root = [
    "poe-pop",
    poePopKindClass(model.kind),
    model.doubleLine ? "poe-pop--double" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");

  const blocks: ReactNode[] = [];
  let needSep = false;
  const sep = (key: string) => (
    <div key={key} className="poe-pop-sep" aria-hidden />
  );

  if (model.properties.length > 0) {
    if (needSep) blocks.push(sep("sep-prop"));
    blocks.push(
      <div key="props" className={styles.block}>
        {model.properties.map((p, i) => (
          <div key={i} className="poe-pop-property">
            {p.label != null && p.value != null ? (
              <>
                {p.label}: <span className="poe-pop-value">{p.value}</span>
              </>
            ) : (
              (p.text ?? p.label ?? p.value)
            )}
          </div>
        ))}
      </div>,
    );
    needSep = true;
  }

  if (model.requirements.length > 0) {
    if (needSep) blocks.push(sep("sep-req"));
    blocks.push(
      <div key="req" className={styles.block}>
        {model.requirements.map((r, i) => (
          <div key={i} className="poe-pop-requirements">
            {r.includes(":") ? (
              <>
                {r.slice(0, r.indexOf(":") + 1)}{" "}
                <span className="poe-pop-value">{r.slice(r.indexOf(":") + 1).trim()}</span>
              </>
            ) : (
              r
            )}
          </div>
        ))}
      </div>,
    );
    needSep = true;
  }

  for (let si = 0; si < model.sections.length; si++) {
    const section = model.sections[si];
    if (needSep) blocks.push(sep(`sep-${si}`));
    if (section.type === "mods") {
      const showSide = section.lines.some((l) => l.side || l.tierLabel);
      blocks.push(
        <div key={`mods-${si}`} className={styles.block}>
          {section.lines.map((line, i) => (
            <ModLineView key={i} line={line} odd={i % 2 === 0} showSide={showSide} />
          ))}
        </div>,
      );
    } else if (section.type === "secDescr") {
      blocks.push(
        <div key={`sec-${si}`} className="poe-pop-sec-descr">
          {section.text}
        </div>,
      );
    } else if (section.type === "flavour") {
      blocks.push(
        <div key={`flav-${si}`} className="poe-pop-flavour">
          {section.text}
        </div>,
      );
    } else {
      blocks.push(
        <div key={`help-${si}`} className="poe-pop-help">
          {section.text}
        </div>,
      );
    }
    needSep = true;
  }

  const { flags } = model;
  if (flags.corrupted || flags.mirrored || flags.sanctified || flags.unidentified) {
    if (needSep) blocks.push(sep("sep-flags"));
    blocks.push(
      <div key="flags" className={styles.block}>
        {flags.unidentified && <div className="poe-pop-help">Unidentified</div>}
        {flags.corrupted && <div className="r-corrupted">Corrupted</div>}
        {flags.sanctified && <div className="r-desecrated">Sanctified</div>}
        {flags.mirrored && <div className="poe-pop-help">Mirrored</div>}
      </div>,
    );
  }

  return (
    <div className={root}>
      <div className={`poe-pop-header${model.doubleLine ? " poe-pop-header--double" : ""}`}>
        {model.doubleLine ? (
          <>
            <div className="poe-pop-name">
              <span className="poe-pop-lc">{model.name}</span>
            </div>
            {model.typeLine ? (
              <div className="poe-pop-name poe-pop-typeLine">
                <span className="poe-pop-lc">{model.typeLine}</span>
              </div>
            ) : null}
          </>
        ) : (
          <span className="poe-pop-lc">{model.name}</span>
        )}
      </div>
      <div className="poe-pop-content">{blocks}</div>
      {showArt && artUrl ? (
        <div className="poe-pop-art">
          {/* eslint-disable-next-line @next/next/no-img-element -- static export */}
          <img
            src={artUrl}
            alt={model.name}
            loading="lazy"
            onError={() => setArtFailed(true)}
          />
        </div>
      ) : null}
    </div>
  );
}
