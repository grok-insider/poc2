# Path of Crafting 2 — PoE2 In-Game Design System

**Goal: the web app must read like a native Path of Exile 2 interface panel**
(the Options / Character / Genesis Tree screens), not like a generic dark web
dashboard. Every panel, button, tooltip and label follows the rules below.
Reference screenshots: the in-game Options panel, Character sheet, Genesis
Tree screen and its node tooltips, plus poe2db.tw's faithful item popups.

## 1. Asset & style provenance (research summary)

| What | Source | Notes |
|---|---|---|
| Item-popup header caps/middles, separators | `cdn.poe2db.tw/image/item/popup2/*`, `image/item/popup/seperator-*` | poe2db mirrors GGG's GGPK UI art; fetched by `fetch-genesis-assets` into `public/genesis-icons/ui/` (regenerable, gitignored) |
| Genesis tree node frames (small/notable × normal/can-allocate/active), womb slot, glow | `cdn.poe2db.tw/image/Art/2DArt/UIImages/InGame/BreachLeague/*` — the **exact in-game assets** named by `BrequelTree.json`'s `art` section | `frame-small-*.webp`, `frame-notable-*.webp`, `frame-womb-slot*.webp`, `node-glow.webp` |
| Fonts | `web.poecdn.com/font/fontin-smallcaps-webfont.woff`, `fontin-regular-webfont.woff` (GGG's own CDN, also used by pathofexile.com) | self-hosted in `public/fonts/`; Fontin is Jos Buivenga freeware |
| Colors / popup metrics | poe2db `stdtheme.css` `:root` variables + computed styles (cross-checked against in-game screenshots) | exact hex values below |
| Panel chrome (gold plaque, bevels) | No extractable flat asset — recreated in CSS per the screenshots | see §4 |

GGG art stays a **regenerable artifact** (never committed); the build degrades
gracefully when assets are missing (system-font fallbacks, plain frames).

## 2. Color tokens (`globals.css :root`)

### Surfaces — "black panel on black world"
| Token | Value | Use |
|---|---|---|
| `--bg-deep` | `#030303` | page void |
| `--bg-base` | `#070605` | app canvas (near-black, warm) |
| `--surface` | `#0c0b09` | panels / cards (in-game panel black) |
| `--surface-2` | `#14110d` | raised wells, hover |
| `--surface-3` | `#1c1813` | pressed / active |
| `--line` | `rgba(178,145,85,.16)` | hairlines — **always warm bronze, never blue/grey** |
| `--line-strong` | `rgba(178,145,85,.32)` | emphasized borders |

### Text
| Token | Value | Use |
|---|---|---|
| `--fg` | `#e8e0d2` | values, primary text (warm white) |
| `--fg-muted` | `#a89a85` | secondary |
| `--fg-faint` | `#6f6354` | tertiary / disabled |
| `--default-color` | `#7f7f7f` | tooltip default text (exact in-game) |

### The gold system (the only "accent")
| Token | Value | Use |
|---|---|---|
| `--gold` | `#b29155` | borders, frames, structural gold |
| `--gold-bright` | `#e7b478` | section titles, plaque text |
| `--gold-action` | `#d29933` | interactive hints ("Click to allocate"), primary CTA |
| `--primary` | alias of `--gold-action` | focus rings, active states |

### Game semantics (exact in-game / poe2db values — never repurpose)
| Token | Value |
|---|---|
| `--r-normal` `#c8c8c8` · `--r-magic` `#8888ff` · `--r-rare` `#ffff77` · `--r-unique` `#ef6916` |
| `--r-currency` `#aa9e82` · `--r-crafted` `#b4b4ff` · `--r-corrupted` `#d20000` · `--r-fractured` `#a29162` · `--r-desecrated` `#a78fbe` |
| `--success` `#46a239` (game green) · `--warn` `#ebc850` · `--danger` `#d20000` |

Negative resistances / dangerous numbers are **red `#ff5555`**, positives
plain white — mirroring the Character sheet.

## 3. Typography

| Role | Font | Rules |
|---|---|---|
| Everything UI | `FontinSmallCaps` (`--font-sans`) | The in-game UI font. Labels render as small caps natively — do **not** add `text-transform: uppercase` on top of it (double-caps); use `letter-spacing: .02–.04em` |
| Prose / tooltips body / flavour | `FontinRegular` (`--font-serif`) | explanatory paragraphs (Hit Damage-style popups), italic flavour text |
| Numbers / code | `Fira Code` (`--font-mono`) | trade ids, probabilities — game has no mono, keep ours for tabular data only |

Sizes: UI base 13–14px, tooltip body 15–16px, item-popup header 19px,
panel plaque 17px. Line-height 1.3 in tooltips (exact in-game metric).

## 4. Component recipes

### Panel ("the black panel")
Black `--surface`, 1px `--line` border, **no border-radius beyond 2–3px**
(the game is square-cornered), inner top highlight
`inset 0 1px 0 rgba(231,180,120,.06)`, deep drop shadow.

### Panel title plaque (Options/Character header)
Centered bar: dark bronze body `linear-gradient(#1c1610, #0b0907)`, 1px
`--gold` border + 1px black outer, title in `--gold-bright`
FontinSmallCaps 17px, letter-spacing .08em, with soft side fades
(`::before/::after` gradients) suggesting the winged ornament. Class:
`.poe-plaque`.

### Section header (Life / Mana / Armour …)
White 15px FontinSmallCaps + full-width 1px underline
`linear-gradient(90deg, var(--gold) 0%, transparent 85%)`. Class:
`.poe-section`.

### Label / value rows (Character sheet)
Label `--fg-muted` smallcaps left, value `--fg` right; row hairline
`rgba(178,145,85,.08)`; negative values `#ff5555`.

### Button (Save / Defaults / Close)
Square-ish (3px), layered metal bevel:
`background linear-gradient(#181410, #0b0a08)`,
`border: 1px solid #000`, `box-shadow: inset 0 0 0 1px rgba(178,145,85,.35), inset 0 1px 0 rgba(255,255,255,.05)`,
white smallcaps. Hover: inner ring → `rgba(231,180,120,.6)`. Primary CTA:
same but gold text `--gold-action`. **No pill buttons.**

### Segmented control / tabs (Graphics·Game·UI·Sound)
Text-only smallcaps `--fg-muted`; active: `--fg` + 2px `--gold` underline.
Container: black well, 1px `--line`, square corners.

### Inputs / sliders
Black well `#050403`, 1px `#3a3328` border, square. Slider: groove =
thin bronze line; thumb = gold square rotated 45° (the in-game diamond).

### Scrollbar
Thin (8px), thumb `#2a241c` with 1px bronze edge — the game's skinny
bronze scrollbar.

### Generic tooltip (Hit Damage / Item Armour style)
`rgba(0,0,0,.92)` body, 1px `#3c3c3c` border, centered white smallcaps
title, body in FontinRegular `--fg-muted`, underlined keywords (dotted,
same color). Max width ~340px.

### Item popup (poe2db 1:1) — class family `.poe-pop*`
Black body; header = 3-layer sprite background
(`ui/header-<rarity>-{left,middle,right}.webp`, `contain`, left/right caps +
repeat-x middle), height 34px (54px doubleLine), name 19px centered, padding
`7px 30px`; rarity name colors per §2; separator = sprite
(`ui/seperator-<rarity>.webp`) centered, 8px high, margin 1px 0; content
centered 16px, line-height 1.3; mod lines `--r-magic`; crafted lines
`--r-crafted`; flavour `--r-unique` italic FontinRegular; "Corrupted"
`--r-corrupted`.

### Genesis node tooltip (Breach style — in-game reference)
Header: dark-red band `linear-gradient(180deg,#2b0a08,#1a0504)` inside a
1px `#000` + inner 1px `#4a1410` double border, title `#ede1c0` 17px
centered; corner claw accents approximated with small radial-gradient
shadows. Body on `rgba(5,3,4,.94)`: mod lines `--r-magic` 15px; italic
hints FontinRegular `--fg-muted`; action hints ("Click to allocate" /
preset why) in `--gold-action`; warnings `--r-corrupted`.

### Genesis tree nodes
Use the **real frames**: `frame-small-normal|canallocate|active.webp`,
`frame-notable-*.webp`, `frame-womb-slot*.webp` under the node icon, plus
`node-glow.webp` behind highlighted nodes. Preset states: step →
`active` frame + glow + priority badge; connector → `canallocate` frame;
optional → `canallocate` + dashed gold ring; avoid → red ✕ + desaturate;
non-preset → `normal` frame dimmed to 35%. Edges: `#3f3257` base,
highlighted `#a78fbe` (the in-game purple links).

## 5. Layout rules

- The console keeps its shell (topbar / rail / canvas / dock) but skinned:
  topbar = black bar with bronze hairline; rail = black column, active icon
  gets a gold left notch; dock = black with bronze top hairline.
- **Genesis page is full-bleed**: the bench (item card) column is hidden;
  layout is `farming (≤280px) | tree (1fr, dominant) | presets (340px)`.
- Density follows the game: compact rows, generous panel padding (14–16px),
  square corners everywhere (`--radius: 3px`, `--radius-sm: 2px`).

## 6. Hard rules

1. **No blue accent.** The old `#7db4ff` is dead; focus/active/primary is
   gold. Blue belongs exclusively to magic-mod text (`--r-magic`).
2. Rarity colors are reserved for game semantics, never decoration.
3. FontinSmallCaps + no `text-transform: uppercase` double-up.
4. Square corners; pills/rounded cards are off-brand.
5. Tooltips/popups are centered-text, black, bordered — never glassmorphic.
6. All GGG art = regenerable fetched assets with graceful fallback.
7. Community numbers stay labeled "community estimate" (gold italic note).
