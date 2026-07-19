# Path of Crafting 2 — UI/UX Redesign ("Forge" console)

> **HISTORICAL — design seed.** This document drove the original "Forge"
> console concept (its Tauri 2 + Svelte 5 target is retired; the layout
> shipped in the Next.js/React web app). The **current UI source of
> truth is [`apps/web/DESIGN.md`](../apps/web/DESIGN.md)** — the PoE2
> in-game design system (Fontin, black/bronze/gold) that superseded the
> violet "Modern Dark" tokens below. Do not implement from this file.

> A fresh, premium, game-aesthetic redesign of the desktop advisor. Grounded in
> the `ui-ux-pro-max` design intelligence:
> **Style** = *Modern Dark (Cinema)* (built for "high-end gaming companion apps,
> pro/trading dashboards"); **Palette** = *Gaming* dark (violet/gold on deep
> indigo-black); **Type** = *Dashboard Data* (Fira Code mono for numbers, Fira
> Sans for labels); **Charts** = *Line-with-Confidence-Band* + *Box/Violin* for
> the Monte-Carlo outputs. Target: Tauri 2 + Svelte 5, 1280×800, dark-only.

---

## 1. Design principles

1. **One primary action, always.** The whole screen orbits a single *Next Best*
   recommendation. Everything else is subordinate. (HIG `primary-action`.)
2. **The bench vs. the guide.** Left = your *item* (state you own). Right = the
   *advisor* (what to do next). A clean mental split, mirrored in layout.
3. **Numbers are first-class.** Every EV, cost, success %, weight and tier uses
   **tabular monospace** so columns never shift and values read like a ledger.
4. **Uncertainty is shown, not hidden.** Success % always travels with its
   Monte-Carlo confidence band; cost always shows a distribution, not a point.
5. **Premium, arcane, calm.** Deep non-pure blacks, frosted glass, hairline
   borders, a single accent glow behind the recommendation — no neon clutter.
6. **Live & legible.** Re-plans on every change with a quiet pulse, never a
   jarring reflow. Color never carries meaning alone (rarity/status get a glyph).

---

## 2. Design tokens

### Color (dark-only; verified for AA contrast on surfaces)
```css
:root {
  /* surfaces — deep, never pure #000 (avoids OLED smear) */
  --bg-deep:    #0A0A0F;   /* app backdrop (with faint radial gradient) */
  --bg-base:    #0F0F16;   /* panels */
  --surface:    #16161F;   /* cards */
  --surface-2:  #1E1E2B;   /* elevated cards / hover */
  --line:       rgba(255,255,255,.08);   /* hairline border */
  --line-strong:rgba(255,255,255,.14);

  /* text */
  --fg:         #E7E7EE;   /* ~13:1 on --surface */
  --fg-muted:   #9AA0B4;   /* ~5.6:1 — secondary */
  --fg-faint:   #6B7088;   /* labels, axis */

  /* brand: arcane violet = ACTIONS/focus; gold = VALUE/currency */
  --primary:        #8B7BFF;
  --primary-press:  #7C6BF0;
  --primary-glow:   rgba(139,123,255,.22);
  --accent:         #E6B450;   /* EV, cost, currency — keeps PoC2's amber heritage */
  --accent-press:   #D9A23E;

  /* status */
  --success: #3DDC97;  --warn: #F5A524;  --danger: #FF5C7A;
  --success-band: rgba(61,220,151,.14);   /* confidence-band fill */

  /* PoE2 rarity / mod semantics (always paired with a glyph, never color-only) */
  --r-normal:#C8C8C8; --r-magic:#7AA6FF; --r-rare:#F4D35E;
  --r-unique:#C9712E; --r-desecrated:#C07AF0; --r-corrupted:#E0473B; --r-fractured:#5BD6C0;

  /* shape & depth */
  --radius:14px;  --radius-sm:10px;  --radius-pill:999px;
  --shadow-card: 0 1px 0 rgba(255,255,255,.04) inset, 0 12px 32px -16px rgba(0,0,0,.7);
  --glow-rec: 0 0 0 1px var(--primary-glow), 0 0 40px -8px var(--primary-glow);
  --easing: cubic-bezier(0.16, 1, 0.3, 1);
}
```

### Typography (`Dashboard Data` pairing)
```css
@import url('https://fonts.googleapis.com/css2?family=Fira+Code:wght@400;500;600;700&family=Fira+Sans:wght@300;400;500;600;700&family=Cinzel:wght@600&display=swap');
--font-sans: 'Fira Sans', system-ui, sans-serif;   /* labels, body, rationale */
--font-mono: 'Fira Code', ui-monospace, monospace; /* ALL numerics, IDs, weights */
--font-display: 'Cinzel', serif;                   /* wordmark + section eyebrows only */
```
Scale: 11 (micro/axis) · 12 (label) · 13 (body) · 15 (card title) · 20 (rec action) ·
28 (hero number). Numbers use `font-variant-numeric: tabular-nums`.

### Motion
- Enter/expand 180–220 ms `--easing`; exit ~120 ms. Press `scale(.98)`.
- List/card entrance staggered 35 ms. Re-plan = 160 ms crossfade + one-shot glow
  pulse on the card that changed. Confidence band wipes in left→right (220 ms).
- `@media (prefers-reduced-motion)` → all transitions collapse to opacity ≤120 ms.

### Effects
- Frosted glass (`backdrop-filter: blur(14px) saturate(120%)`) on the **top bar**,
  the **bottom dock**, and **modals/sheets** only — not on every card.
- Hairline `--line` borders everywhere; `--shadow-card` on cards; `--glow-rec`
  only on the single primary recommendation.
- One faint background radial (`radial-gradient(1200px 600px at 70% -10%,
  rgba(139,123,255,.06), transparent)`) — the "arcane ambience", no animated blobs
  (calm > busy on a tool).

---

## 3. Information architecture & layout

A focused **3-zone "Crafting Console"** (replaces drawer-tab navigation with a
persistent split): a 64 px icon rail, a 2-column canvas (Bench | Guide), and a
frosted ledger dock.

```
┌────────────────────────────────────────────────────────────────────────────────┐
│ ⬡ PATH OF CRAFTING        Vaal Regalia · Rare · ilvl 82      League ▾  ⟳ 4m  ⚙ │  top bar (glass)
├────┬───────────────────────────────────────┬──────────────────────────────────┤
│ ▣  │  THE BENCH  (your item)               │  THE GUIDE  (next best move)      │
│ ◎  │ ┌───────────────────────────────────┐ │ ┌──────────────────────────────┐ │
│ ⚑  │ │ ◆ Vaal Regalia                    │ │ │ NEXT BEST            73% ▲    │ │  hero rec
│ ▦  │ │ Rare · Energy Shield · ilvl 82    │ │ │ Perfect Orb of Transmutation │ │  (glow)
│ ⌗  │ │ Prefix ◈ ◇ ◇     Suffix ◈ ◇ ◇    │ │ │ +1 Tier-1 ES prefix          │ │
│ ⚙  │ │ ───── prefixes ─────              │ │ │ ▓▓▓▓▓▓▓░░  68 – 78 %  band   │ │
│    │ │ ◈ +118 Energy Shield      T2  ✓   │ │ │ ~12.4 div  · 1 step          │ │
│    │ │ ◇ (empty)                         │ │ │ via R232 “ilvl 82 unlocks T1”│ │
│    │ │ ───── suffixes ─────              │ │ │              [  Apply ▸  ]    │ │
│    │ │ ◈ +35% Fire Resistance    T3      │ │ └──────────────────────────────┘ │
│    │ │ ◈ +28% Cold Resistance    T4  ●   │ │  ALTERNATIVES                     │
│    │ └───────────────────────────────────┘ │  ▸ Exalted Orb       61%  ~8.0d   │
│    │  TARGET                               │  ▸ Regal Orb         55%  ~3.1d   │
│    │  3× Energy Shield ◈◈◇  2× Resist ◈◈  │  ▸ Chaos Orb         48%  ~6.4d   │
│    │  Budget ▰▰▰▰▱▱▱▱  40 / 100 div        │ ┌ Eligible · Distribution · Why ┐ │  sub-tabs
│    │                                       │ │  (chart / pool / trace)        │ │
├────┴───────────────────────────────────────┴──────────────────────────────────┤
│  spent 40.0d  ·  next ~12.4d  ·  projected total ~52d     risk ▰▰▱  step 4  [ Record outcome ▾ ] │  dock (glass)
└────────────────────────────────────────────────────────────────────────────────┘
```

**Rail icons** (Lucide, 1.75 stroke, single family): Item `box`, Target `crosshair`,
Guide `wand-2`, Eligible `layers`, History `history`, Database `database`, Tools
`flask-conical`, Settings `settings`. Active = violet pill + left accent bar.

---

## 4. Component redesigns

### 4.1 Item card — "the bench"
A poe2db-style tooltip, modernised. Header tinted by **rarity** (`--r-rare` left
border + glyph ◆). **Affix capacity as diamonds:** filled ◈ = occupied, hollow ◇ =
open, so prefix/suffix room reads at a glance. Each mod row: `◈ value  Tier  flag`
— value in mono, tier as a small chip, flags as glyphs (● fractured, ✦ desecrated,
☠ corrupted) so meaning never relies on color. Mods that satisfy a target concept
get a left `--success` tick; off-target mods stay neutral. Implicit + corrupted
shown in a divided footer band.

### 4.2 Recommendation hero — "next best"
The emotional center. `--glow-rec` ring, violet eyebrow "NEXT BEST", the
**action** at 20 px, a **success % at 28 px** with a ▲/▼ delta vs. the previous
plan. Below it the **confidence band** (a slim horizontal bar: solid fill =
point estimate, translucent `--success-band` = the Monte-Carlo 68–78 % range,
ticks at the bounds). A value line in gold: `~12.4 div · N steps`. A
**traceability chip**: `via R232` / strategy step — click to open *Why*. One gold
`Apply ▸` button (the only filled button on screen). Trained-policy badge (⚛) when
the Q-model drove the pick.

### 4.3 Alternatives list
Compact rows, ranked: `▸ action … success% … ~cost`. Hover lifts to `--surface-2`
+ shows a mini band. Click promotes it into the hero (with a shared-element slide)
so you can compare without losing place. Guidance-only tips render as a muted
`ⓘ advice` row at the bottom, never ranked above a concrete step.

### 4.4 Target planner
Inline on the bench, not a separate screen. Concept **chips** with a stepper for
count and a tier dropdown (`T1 / ≥T2 / any`); prefix vs suffix grouped. The
**budget** is a segmented meter (min ▰ / expected ▰ / max ▱) that fills as `spent`
grows and turns `--warn` past expected, `--danger` past max. "OR" groups
(`Fire|Cold|Lightning Res`) shown as a single chip with a `∨` joiner.

### 4.5 Sub-panel tabs (under the guide)
- **Eligible** — the rollable mod pool for the next action: a virtualized table
  (`tier · mod · weight · % chance`), sortable, with the inclusive-weight and
  ilvl-gate made explicit (a faint "unlocked at ilvl 82" caption). Bulk-filter by
  affix/concept chips.
- **Distribution** — the outcome viz (see §5).
- **Why** — the full trace: which rule/strategy fired, the predicate that matched,
  the EV math expanded, and the recovery branch if it fails.

### 4.6 Cost ledger dock (frosted, fixed)
A single line: `spent · next · projected total` (gold mono), a 3-segment **risk**
bar, the **step counter**, and the primary `Record outcome ▾` split-button (success
/ failure / custom roll). Expanding it slides up a *Required materials* shopping
list + the compact crafting log.

### 4.7 Recovery (after a failure)
When the last step failed, the hero card flips to a `--danger`-tinted
**"Recovery"** state: the default fallback action + 1–3 branch options with cost
deltas (`+4d`, `+0d`), each a one-click jump. The item card briefly flashes the
mod that was lost/changed.

### 4.8 Simulation runner & Database
Tools rail → a sheet: run the current recommendation N× and render the cost/result
distribution live (streaming histogram). Database → a full-screen inspector with a
left filter rail (class/material) and poe2db-style detail cards; doubles as the
"pick a base" entry to start a fresh craft.

---

## 5. Data visualization

| Where | Chart | Why (from the chart domain) |
|---|---|---|
| Hero success % | **Confidence band bar** | Communicates the Monte-Carlo uncertainty range, not a false-precision point. Solid = estimate, 14 %-opacity fill = 68–78 % band. |
| Distribution tab | **Box / violin plot** of cost (div) | Shows min / Q1 / median / Q3 / max + outliers across the simulated trials — exactly the budget min/expected/max story. Always paired with a stats summary table (a11y fallback). |
| Multi-step plan | **Line + confidence band** of cumulative cost vs. step | Actual solid, projected dashed, band = spread. Horizon ≤30 % of x-range. |
| Sim runner | **Streaming histogram** | Outcomes accrue live; bars use status colors + a pattern fill so they're distinguishable without color. |

Charts: legends always visible, tooltips on hover, axes labelled with units, grid
lines low-contrast (`--line`), tabular number formatting, reduced-motion respected.

---

## 6. States & accessibility

- **Empty states** with guidance: no bundle → "Build a data bundle" CTA; no item →
  "Paste an item (Ctrl-C in PoE2) or pick a base"; no target → example goal chip.
- **Loading**: skeleton shimmer for the recommendation + eligible table when a plan
  takes >300 ms; never a blank panel.
- **Contrast**: body ≥4.5:1, secondary ≥3:1, focus ring 2 px `--primary` always
  visible. Rarity/status meaning is always glyph + text, never color alone.
- **Keyboard**: full tab order matches layout; `A` apply, `R` record, `1–5` pick
  alternative, `/` focus item-paste. Reduced-motion + system text-scaling supported.
- **No emoji** as icons — one Lucide family, 1.75 stroke, sized via tokens.

---

## 7. Implementation notes (Svelte 5 + Tauri)

- Ship tokens as a single `:root` stylesheet (above) + Svelte scoped styles;
  reference only `var(--token)` in components (no raw hex). One `theme.css`.
- Svelte 5 runes: `$state` item/goal, `$derived` for the affix-capacity diamonds &
  budget meter, `$effect` to re-`recommend` on change (debounced 150 ms) — the
  existing Tauri commands (`recommend`, `eligible_mods`, `run_n_trials`,
  `recovery_hints`) stay unchanged; this is a **view-layer** redesign.
- Charts: a lightweight lib (LayerCake or uPlot for Svelte) for the band + box
  plots; the histogram can be hand-rolled divs for the streaming case.
- Fonts self-hosted (no runtime network) — vendor Fira Code/Sans + Cinzel into
  `apps/web/public/fonts/`. (This redesign shipped in the React/Next.js web app
  `apps/web`, not the original Svelte desktop app; the notes below predate that.)
- Migrate incrementally: introduce `theme.css` + the 3-zone `App.svelte` shell
  first (hero rec + item card), then port each panel into the new tabs.

---

## 8. What changes vs. today

- Drawer-tab navigation → persistent **Bench | Guide** split with a slim icon rail
  (less hunting, the recommendation is always visible).
- Flat amber-on-dark → a layered **arcane-violet + gold** system with rarity
  semantics, frosted glass, and a single focused glow.
- Point-estimate success/cost → **uncertainty-first** (confidence band + box plot).
- Generic dashboard feel → a **game-grade crafting console** with a clear emotional
  center and a ledger that reads like a craft log.
