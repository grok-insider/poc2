"use client";

import { useCraft } from "@/lib/store";
import { baseIconUrl } from "@/lib/baseIcons";
import { rarityClass } from "@/lib/format";
import type { Rarity } from "@/lib/types";
import styles from "./BaseIcon.module.css";

/// Renders a base-item icon by its bundle base id, falling back to a
/// rarity-tinted letter glyph when the scraped icon isn't available.
export function BaseIcon({
  baseId,
  name,
  rarity = "normal",
  size = 40,
}: {
  baseId?: string | null;
  name?: string | null;
  rarity?: Rarity;
  size?: number;
}) {
  const manifest = useCraft((s) => s.iconManifest);
  const url = baseIconUrl(manifest, baseId);
  const style = { width: size, height: size };

  if (url) {
    return (
      // eslint-disable-next-line @next/next/no-img-element -- static export, images.unoptimized
      <img
        src={url}
        alt={name ?? "base item"}
        width={size}
        height={size}
        loading="lazy"
        className={styles.icon}
        style={style}
      />
    );
  }

  const letter = (name ?? "?").trim().charAt(0).toUpperCase() || "?";
  return (
    <span
      className={`${styles.fallback} ${rarityClass(rarity)}`}
      style={style}
      aria-hidden
      title={name ?? undefined}
    >
      {letter}
    </span>
  );
}
