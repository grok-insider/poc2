"use client";

/// Vendor tab — shopping filters for vendor/stash search: item class,
/// rarity, ilvl/level ranges, quality/sockets, movement speed, resists,
/// attributes, and common mod families. All hand-authored micro-patterns
/// (lib/regex/vendor.ts).

import { useMemo } from "react";
import {
  VENDOR_CLASSES,
  vendorTerms,
  type VendorClass,
  type VendorSettings,
} from "@/lib/regex/vendor";
import { useRegex } from "@/lib/regex/state";
import { RegexResult } from "./RegexResult";
import styles from "./RegexPanel.module.css";

const MOVEMENT_SPEEDS = [10, 15, 20, 25, 30, 35];

function Check({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className={styles.check} data-on={checked}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
  );
}

export function RegexVendorTab() {
  const vendor = useRegex((s) => s.vendor);
  const setVendor = useRegex((s) => s.setVendor);

  const terms = useMemo(() => vendorTerms(vendor), [vendor]);

  function patch(p: Partial<VendorSettings>) {
    setVendor({ ...vendor, ...p });
  }

  function toggleClass(c: VendorClass, on: boolean) {
    patch({
      classes: on ? [...vendor.classes, c] : vendor.classes.filter((x) => x !== c),
    });
  }

  function toggleSpeed(v: number, on: boolean) {
    patch({
      movementSpeeds: on
        ? [...vendor.movementSpeeds, v]
        : vendor.movementSpeeds.filter((x) => x !== v),
    });
  }

  const num = (v: string) => Math.max(0, Math.floor(Number(v) || 0));

  return (
    <div className={styles.stack}>
      <RegexResult terms={terms} mode="all" />

      <section className={`card ${styles.section}`}>
        <span className="eyebrow">Item</span>
        <div className={styles.groupGrid}>
          <Check
            label="Normal"
            checked={vendor.rarity.normal}
            onChange={(v) => patch({ rarity: { ...vendor.rarity, normal: v } })}
          />
          <Check
            label="Magic"
            checked={vendor.rarity.magic}
            onChange={(v) => patch({ rarity: { ...vendor.rarity, magic: v } })}
          />
          <Check
            label="Rare"
            checked={vendor.rarity.rare}
            onChange={(v) => patch({ rarity: { ...vendor.rarity, rare: v } })}
          />
          <Check label="Quality" checked={vendor.quality} onChange={(v) => patch({ quality: v })} />
          <Check label="Sockets" checked={vendor.sockets} onChange={(v) => patch({ sockets: v })} />
        </div>
        <div className={styles.rangeRow}>
          <span className="field-label">Item level</span>
          <input
            className="field num"
            placeholder="min"
            inputMode="numeric"
            value={vendor.itemLevel.min || ""}
            onChange={(e) => patch({ itemLevel: { ...vendor.itemLevel, min: num(e.target.value) } })}
          />
          <span className="faint">–</span>
          <input
            className="field num"
            placeholder="max"
            inputMode="numeric"
            value={vendor.itemLevel.max || ""}
            onChange={(e) => patch({ itemLevel: { ...vendor.itemLevel, max: num(e.target.value) } })}
          />
          <span className="field-label">Requires level</span>
          <input
            className="field num"
            placeholder="min"
            inputMode="numeric"
            value={vendor.requiresLevel.min || ""}
            onChange={(e) =>
              patch({ requiresLevel: { ...vendor.requiresLevel, min: num(e.target.value) } })
            }
          />
          <span className="faint">–</span>
          <input
            className="field num"
            placeholder="max"
            inputMode="numeric"
            value={vendor.requiresLevel.max || ""}
            onChange={(e) =>
              patch({ requiresLevel: { ...vendor.requiresLevel, max: num(e.target.value) } })
            }
          />
        </div>
      </section>

      <section className={`card ${styles.section}`}>
        <span className="eyebrow">Item class</span>
        <div className={styles.groupGrid}>
          {VENDOR_CLASSES.map((c) => (
            <Check
              key={c}
              label={c}
              checked={vendor.classes.includes(c)}
              onChange={(v) => toggleClass(c, v)}
            />
          ))}
        </div>
      </section>

      <section className={`card ${styles.section}`}>
        <span className="eyebrow">Movement speed</span>
        <div className={styles.groupGrid}>
          {MOVEMENT_SPEEDS.map((v) => (
            <Check
              key={v}
              label={`${v}%`}
              checked={vendor.movementSpeeds.includes(v)}
              onChange={(on) => toggleSpeed(v, on)}
            />
          ))}
        </div>
      </section>

      <section className={`card ${styles.section}`}>
        <span className="eyebrow">Resistances &amp; attributes</span>
        <div className={styles.groupGrid}>
          <Check
            label="Fire res"
            checked={vendor.resists.fire}
            onChange={(v) => patch({ resists: { ...vendor.resists, fire: v } })}
          />
          <Check
            label="Cold res"
            checked={vendor.resists.cold}
            onChange={(v) => patch({ resists: { ...vendor.resists, cold: v } })}
          />
          <Check
            label="Lightning res"
            checked={vendor.resists.lightning}
            onChange={(v) => patch({ resists: { ...vendor.resists, lightning: v } })}
          />
          <Check
            label="Chaos res"
            checked={vendor.resists.chaos}
            onChange={(v) => patch({ resists: { ...vendor.resists, chaos: v } })}
          />
          <Check
            label="Strength"
            checked={vendor.attributes.strength}
            onChange={(v) => patch({ attributes: { ...vendor.attributes, strength: v } })}
          />
          <Check
            label="Dexterity"
            checked={vendor.attributes.dexterity}
            onChange={(v) => patch({ attributes: { ...vendor.attributes, dexterity: v } })}
          />
          <Check
            label="Intelligence"
            checked={vendor.attributes.intelligence}
            onChange={(v) => patch({ attributes: { ...vendor.attributes, intelligence: v } })}
          />
          <Check
            label="All attributes"
            checked={vendor.attributes.all}
            onChange={(v) => patch({ attributes: { ...vendor.attributes, all: v } })}
          />
        </div>
      </section>

      <section className={`card ${styles.section}`}>
        <span className="eyebrow">Mods</span>
        <div className={styles.groupGrid}>
          <Check
            label="Max life"
            checked={vendor.mods.life}
            onChange={(v) => patch({ mods: { ...vendor.mods, life: v } })}
          />
          <Check
            label="Max mana"
            checked={vendor.mods.mana}
            onChange={(v) => patch({ mods: { ...vendor.mods, mana: v } })}
          />
          <Check
            label="Spirit"
            checked={vendor.mods.spirit}
            onChange={(v) => patch({ mods: { ...vendor.mods, spirit: v } })}
          />
          <Check
            label="Item rarity"
            checked={vendor.mods.rarity}
            onChange={(v) => patch({ mods: { ...vendor.mods, rarity: v } })}
          />
          <Check
            label="Physical damage"
            checked={vendor.mods.physicalDamage}
            onChange={(v) => patch({ mods: { ...vendor.mods, physicalDamage: v } })}
          />
          <Check
            label="Spell damage"
            checked={vendor.mods.spellDamage}
            onChange={(v) => patch({ mods: { ...vendor.mods, spellDamage: v } })}
          />
          <Check
            label="Attack speed"
            checked={vendor.mods.attackSpeed}
            onChange={(v) => patch({ mods: { ...vendor.mods, attackSpeed: v } })}
          />
          <Check
            label="Cast speed"
            checked={vendor.mods.castSpeed}
            onChange={(v) => patch({ mods: { ...vendor.mods, castSpeed: v } })}
          />
          <Check
            label="Adds fire dmg"
            checked={vendor.mods.addsFire}
            onChange={(v) => patch({ mods: { ...vendor.mods, addsFire: v } })}
          />
          <Check
            label="Adds cold dmg"
            checked={vendor.mods.addsCold}
            onChange={(v) => patch({ mods: { ...vendor.mods, addsCold: v } })}
          />
          <Check
            label="Adds lightning dmg"
            checked={vendor.mods.addsLightning}
            onChange={(v) => patch({ mods: { ...vendor.mods, addsLightning: v } })}
          />
          <Check
            label="Adds chaos dmg"
            checked={vendor.mods.addsChaos}
            onChange={(v) => patch({ mods: { ...vendor.mods, addsChaos: v } })}
          />
          <Check
            label="+ all skills"
            checked={vendor.mods.skillsAll}
            onChange={(v) => patch({ mods: { ...vendor.mods, skillsAll: v } })}
          />
          <Check
            label="+ minion skills"
            checked={vendor.mods.skillsMinion}
            onChange={(v) => patch({ mods: { ...vendor.mods, skillsMinion: v } })}
          />
          <Check
            label="+ melee skills"
            checked={vendor.mods.skillsMelee}
            onChange={(v) => patch({ mods: { ...vendor.mods, skillsMelee: v } })}
          />
          <Check
            label="+ projectile skills"
            checked={vendor.mods.skillsProjectile}
            onChange={(v) => patch({ mods: { ...vendor.mods, skillsProjectile: v } })}
          />
          <Check
            label="+ spell skills"
            checked={vendor.mods.skillsSpell}
            onChange={(v) => patch({ mods: { ...vendor.mods, skillsSpell: v } })}
          />
        </div>
      </section>
    </div>
  );
}
