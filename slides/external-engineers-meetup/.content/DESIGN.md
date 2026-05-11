# DESIGN.md - From Code Search to AI-Powered Research Engine

> Refresh: design system rebuilt for stronger UX, hierarchy, and act structure. Theme name: **Blueprint Console** — dark technical keynote that feels like a live debugger for reasoning, not another AI pitch.

## Visual identity
**Mood:** Dark, precise, evidence-oriented. Each slide should read in 3 seconds without the speaker.
**Inspiration:** Code editors, debugger panels, protocol traces, observability dashboards.
**Distinctive choice:** IBM Plex Mono as the heading voice — slide titles read like protocol statements, not marketing copy.

## Color system
| Token | Value | Role |
|-------|-------|------|
| `--bg` / `--bg-2` | `#07101f` / `#0a1628` | Slide background gradient base |
| `--surface` / `--surface-2` | `#0f1d35` / `#142847` | Card / panel backgrounds (gradient pair) |
| `--border` / `--border-strong` | `#1f3358` / `#2c4778` | Dividers, panel edges |
| `--accent` / `--accent-2` / `--accent-strong` | `#5cd0ec` / `#a6e8f2` / `#38b9da` | Primary cyan — focal element only |
| `--warm` | `#f0a45c` | Discomfort / problem signals (Act I, "would-do-differently") |
| `--positive` | `#6ee7a7` | Verified / held / success badges |
| `--violet` | `#a78bfa` | Secondary accent (LSP, planner, Act II marker) |
| `--negative` | `#f08585` | Reserved for failure callouts |
| `--code-bg` / `--code-text` | `#050d1c` / `#d7ecff` | Code panels |

The accent rule: **at most one** focal accent per slide; warm/positive/violet are used contextually to mark act, evidence-status, or problem/solution lanes.

## Typography
| Token | Font | Use |
|-------|------|-----|
| `--font-head` | IBM Plex Mono 500–700 | Display, titles, eyebrows, badges, code |
| `--font-body` | IBM Plex Sans 400–600 | Subtitles, bullets, captions |
| `--font-mono` | IBM Plex Mono 500 | Code panels and the title envelope |

Type scale is fixed for the 1280x720 stage so rendered slides do not scroll inside the iframe. Reading order is enforced by hierarchy, not size alone.

## Layout system
The deck uses **11 distinct layout types** across 30 slides — no two consecutive slides share the same shape, and the act structure is visible by layout rhythm:

| Type | Slides | Purpose |
|------|--------|---------|
| `slide--title` | 01 | Hero with code envelope on the right |
| `slide--comparison` | 02, 14 | Before/after, planner/researcher |
| `slide--timeline` | 03 | Three-step origin with linked dots |
| `slide--content` (with `with-aside`) | 04, 12, 26, 28 | Bullet stack + aside callout |
| `slide--flow` | 05, 10, 16, 19, 21, 23, 25 | Horizontal/vertical chains with SVG arrows |
| `slide--cards` | 06, 08, 15, 18, 20, 22 | Thematic cards (color-coded) |
| `slide--code` | 07, 11, 17, 21 | Code panel with `data-label` chrome and inline syntax tokens |
| `slide--architecture` | 13 | Four anchored stages with rail line |
| `slide--matrix` | 24, 27, 29 | Decision rows with badges |
| `slide--closing` | 30 | Distinct CTA (`code search → code intelligence → research engine`) |

## Act structure (visible to the audience)
Three eyebrow markers create momentum without adding slides:

- Slide 02 — `eyebrow--warm` "Act I · The problem"
- Slide 12 — `eyebrow--violet` "Act II · The system"
- Slide 24 — `eyebrow--violet` "Act III · Day to day"

The closing slide (30) returns to `eyebrow--positive`.

## Visual differentiation mechanisms
1. **Background:** subtle radial accent + dot grid (masked at top/bottom) — replaces the same gradient on every slide
2. **Cards:** top-edge color stripe selects the lane (cyan / warm / positive / violet)
3. **Flow nodes:** `node--accent` highlights the focal step; `node--positive` marks the outcome
4. **Matrix rows:** left-border color encodes the row's status
5. **Bullets:** rotated diamond markers with subtle glow (warm/positive variants for problem/solution)
6. **Eyebrow pip:** glowing dot before each kicker, color-tied to act/state
7. **Code panels:** `data-label` corner tag identifies the contract; inline `tk-*` tokens give real syntax color
8. **Arrows:** CSS-drawn line + chevron (replaces literal `->` text)

## Animation approach
- Eight-step stagger (`delay-1` … `delay-8`) so multi-node flows animate in sequence
- Three entry types: `fade-in`, `slide-up`, `scale-in` — used contextually
- All animation honours `prefers-reduced-motion: reduce`

## Pointer & click feedback
A subtle layer of pointer chrome makes the deck feel like a live console — the speaker's cursor and clicks become part of the reasoning trace, not a separate UI.

| Element | Behaviour | Token |
|---------|-----------|-------|
| **Custom cursor** | Replace the default arrow with a small cyan ring + center dot; the ring lags the dot by ~80 ms (spring) for a debugger-feel pointer. Default OS cursor stays available on form fields, links, and during text selection. | `--accent` ring, `--accent-2` dot |
| **Hover state** | Ring scales to 1.6× and shifts to `--accent-strong` over interactive elements (buttons, cards, code panels, flow nodes). | `--accent-strong` |
| **Mouse-down spark** | On `pointerdown`, emit a 6-spoke radial spark at the click point — short (320 ms), `cubic-bezier(0.22, 1, 0.36, 1)`, drawn in `--accent` with one `--violet` ray for the focal lane. Honours `prefers-reduced-motion: reduce` (degrades to a flat 1-frame ring). | `--accent`, `--violet` |
| **Iframe handoff** | The parent (`index.html`) owns one shared overlay so the cursor stays continuous across slide transitions; chrome (HUD, progress bar, counter) is excluded from spark hits. | — |

Driven by `pointermove` / `pointerdown` on the parent window, with a `pointer: coarse` short-circuit so touch devices opt out cleanly. Disabled in overview mode (`body.overview`) — the grid uses the native cursor for thumbnail clicks.

## Libraries
The deck stays dependency-free for content (syntax colouring uses inline `<span class="tk-*">` tokens against the theme palette — no JS highlighter, no CDN). Two micro-libraries are vendored locally for pointer chrome only:

| Library | Size | Why | Local path |
|---------|------|-----|------------|
| **[tholman/cursor-effects](https://github.com/tholman/cursor-effects)** (MIT) | ~3 KB ESM | Battle-tested `followingDotCursor` / `trailingCursor` we can theme via `--accent`. Already respects `prefers-reduced-motion`, matching the deck's existing animation policy. Imported as `import { followingDotCursor } from './vendor/cursor-effects.esm.js'`. | `js/vendor/cursor-effects.esm.js` |
| **[hexagoncircle/click-spark](https://github.com/hexagoncircle/click-spark)** (MIT) | ~1 KB | Single Web Component `<click-spark>` for the mouse-down spark. Themed via `--click-spark-color: var(--accent)`. Drop-in `<script type="module">`, no build step. | `js/vendor/click-spark.js` |

Both are loaded from `js/vendor/` (offline-friendly, no CDN dependency at present time), wrapped behind a tiny `js/cursor-fx.js` adapter so the rest of the deck only sees CSS variables — never library APIs.
