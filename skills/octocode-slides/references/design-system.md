# Design System Reference — Octocode Slides

Read at Phase 4 start (before any design decisions).

---

## CSS Variable Contract

`scripts/base.css` (copied verbatim to `css/base.css`) defines these variables. `theme.css` overrides them. Slide HTML uses them via `var(...)`. No slide ever sets a raw color, font-family, or font-size.

```
--bg          slide background
--surface     card / panel backgrounds
--border      dividers, code block edges
--accent      one focal element per slide
--text        body text
--muted       captions, metadata, labels
--code-bg     code block background
--code-text   code block text
--font-head   heading font stack
--font-body   body font stack
--font-mono   monospace font stack
```

Typography scale (defined in base.css using `clamp()` only):
`--t-display  --t-title  --t-sub  --t-body  --t-small  --t-code`

Calibration bands:

| Token | Target range | Use |
|-------|-------------|-----|
| `--t-display` | 48–64pt | Title slide headline |
| `--t-title` | 28–44pt | Slide heading |
| `--t-sub` | 22–28pt | Sub-heading, key number |
| `--t-body` | 18–24pt | Bullets, paragraphs |
| `--t-small` | 14–18pt | Captions, source lines |
| `--t-code` | 14–18pt | Code blocks |

---

## Slide Title Contract

Claim-sentence vs topic-label rule lives in `slide-rules.md §1.2` (master rule set) and is summarised in `SKILL.md → Operating principles`. This file does not restate it — but every CSS decision below assumes titles carry meaning, not labels.

---

## Design Reasoning

The reasoning chain that drives every aesthetic decision lives in `references/04-design.md → Design Reasoning Chain`. Walk that chain *before* opening this document — the answers there determine which sections of the design system you actually need.

This file is the visual *contract*: tokens, palettes, typography presets, font pairings. It is not the *process*.

---

## Theme Authoring — rules of thumb

The Phase 4 process is in `04-design.md`; this section captures the rules that process applies. Use these alongside the Design Reasoning Chain — not as a replacement.

**Audience signals → visual posture (a starting frame, not a verdict):**

| Audience | What it suggests |
|----------|-----------------|
| Developers / technical | High contrast, code-native fonts, readable from distance |
| Executives / investors | Premium restraint, serif or distinctive sans, limited palette |
| Creative / product | More expressive color and layout, personality over safety |
| Academic / research | Clean, readable, data-forward — legibility over style |
| Mixed / public | Light background, clear hierarchy, no niche aesthetics |

**Font pairing rules:**
- Two fonts maximum (one heading, one body). Monospace adds a third for code.
- Heading font should have *character* — chosen, not defaulted (Inter / Roboto alone fail Visual Slop).
- Body font legible at 18–22pt on screen.
- The pair should feel related: both geometric, both humanist, or contrast intentionally (serif heading + sans body).

**Color order — pick in this sequence so contrast and hierarchy fall out naturally:**
1. `--bg` (the base — sets the entire mood)
2. `--text` (contrast ≥ 4.5:1 against `--bg` — non-negotiable)
3. `--accent` (one focused punch; used on ≤3 elements per slide)
4. `--surface`, `--border`, `--muted` (usually derived from `--bg`)
5. `--code-bg`, `--code-text` (readable mono colors that feel cohesive with the theme)

Use OKLCH for dark/muted themes to get perceptually uniform lightness steps.
Validate every `--text` / `--bg` pair at `https://webaim.org/resources/contrastchecker`.

---

## Theme Selection Matrix

When a user gives a strong context signal, use this as a starting-point compass — not a final answer:

| Audience | Explore these aesthetics first |
|----------|-----------------------------|
| Developers / engineers | Dark + monospace or geometric sans, cool accent |
| Business / executives | Dark premium or editorial light, warm or neutral |
| Designers / creatives | Warm editorial, expressive type, strong accent |
| Academics / researchers | Light, serif heading, data-forward, restrained accent |
| Marketing / product | High-energy color, bold display font, modern sans |
| Mixed / general | Light background, legible sans, subtle accent |

---

## Font Pairing Presets

Validated pairings across common contexts. All on Google Fonts or Fontshare (free).

| Context | Heading | Body | Note |
|---------|---------|------|------|
| Modern corporate | Outfit | Source Sans 3 | Clean, not generic |
| Startup pitch | Poppins Bold | DM Sans | Friendly confidence |
| Creative / editorial | Fraunces | Plus Jakarta Sans | Striking contrast |
| Technical / data | IBM Plex Mono | IBM Plex Sans | Cohesive family |
| Warm editorial | Instrument Serif | Sora | Uncommon serif presence |
| Dark keynote | Cabinet Grotesk ExtraBold | General Sans | Display weight impact |
| Research / academic | Lora | Inter | Authoritative + readable |
| Dev talk | JetBrains Mono | Space Grotesk | Code-native personality |

Treat these as starting points for research, not the only options.

---

## Curated Palette Library

Five battle-tested palettes with validated WCAG AA contrast ratios. Use these as a starting point for Phase 4 or as direct overrides when the brief gives a strong context signal. Validate modified values at `https://webaim.org/resources/contrastchecker` before shipping when contrast is uncertain.

---

### Graphite — dark cool · developers / tech stage
High contrast, code-native. Works on dark projectors. Cool-blue accent reads clearly at distance.

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `#0F1117` | Slide background |
| `--surface` | `#161B25` | Card / panel |
| `--border` | `#24293A` | Dividers |
| `--accent` | `#5A9EFF` | Focal element — contrast **5.2:1** on `--bg` ✓ |
| `--text` | `#E4E9F2` | Body text — contrast **14.1:1** on `--bg` ✓ |
| `--muted` | `#6B7694` | Captions |
| `--code-bg` | `#0A0E16` | Code blocks |
| `--code-text` | `#79C0FF` | Code text |

Recommended pair: **Space Grotesk ExtraBold** / IBM Plex Sans

---

### Cream — light warm · business / editorial / product
Premium, print-friendly. Terracotta accent feels chosen, not defaulted. Passes easily in bright rooms.

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `#FAF9F6` | Slide background |
| `--surface` | `#F0EDE6` | Card / panel |
| `--border` | `#E0DDD4` | Dividers |
| `--accent` | `#A84427` | Focal element — contrast **5.8:1** on `--bg` ✓ |
| `--text` | `#1C1916` | Body text — contrast **17.4:1** on `--bg` ✓ |
| `--muted` | `#8A867E` | Captions |
| `--code-bg` | `#EDE9E0` | Code blocks |
| `--code-text` | `#7A3218` | Code text |

Recommended pair: **Fraunces** / Plus Jakarta Sans

---

### Midnight — dark warm · keynote / executive / premium pitch
Warm purple-black base with gold accent. Feels deliberate and high-end. Differentiates from standard dark themes.

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `#100E17` | Slide background |
| `--surface` | `#1A1726` | Card / panel |
| `--border` | `#2C2840` | Dividers |
| `--accent` | `#C9A86C` | Focal element — contrast **7.3:1** on `--bg` ✓ |
| `--text` | `#EDE8F5` | Body text — contrast **15.4:1** on `--bg` ✓ |
| `--muted` | `#7A7090` | Captions |
| `--code-bg` | `#0C0A12` | Code blocks |
| `--code-text` | `#E8D5A3` | Code text |

Recommended pair: **Cabinet Grotesk ExtraBold** / General Sans

---

### Blueprint — dark technical · data / architecture / engineering
Deep navy with muted teal. Signals technical credibility without the neon-dashboard cliché. Good for diagram-heavy decks.

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `#0B1220` | Slide background |
| `--surface` | `#101A30` | Card / panel |
| `--border` | `#1E2E4A` | Dividers |
| `--accent` | `#4EC9E0` | Focal element — contrast **5.7:1** on `--bg` ✓ |
| `--text` | `#D8E3F0` | Body text — contrast **13.5:1** on `--bg` ✓ |
| `--muted` | `#6080A8` | Captions |
| `--code-bg` | `#071020` | Code blocks |
| `--code-text` | `#85C9E8` | Code text |

Recommended pair: **IBM Plex Mono** / IBM Plex Sans

---

### Nordic — light cool · academic / research / mixed audience
Soft off-white base with deep navy accent. Universally readable, projects well, passes AAA on text.

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `#F5F6FA` | Slide background |
| `--surface` | `#EAECF4` | Card / panel |
| `--border` | `#D4D8E8` | Dividers |
| `--accent` | `#2F4FA8` | Focal element — contrast **8.1:1** on `--bg` ✓ WCAG AAA |
| `--text` | `#1A1D2E` | Body text — contrast **16.3:1** on `--bg` ✓ |
| `--muted` | `#7880A0` | Captions |
| `--code-bg` | `#E0E4F0` | Code blocks |
| `--code-text` | `#1A3A80` | Code text |

Recommended pair: **Lora** / Source Sans 3

---

## Design Rules

### Typography
- Heading size: `--t-display` on title/section slides only; `--t-title` everywhere else.
- Body text below `--t-body` (≈18pt): split the slide instead of shrinking the font.
- Line height: `1.12` for display, `1.25` for headings, `1.6` for body.
- Letter spacing: `-0.02em` on display/title, `0` on body.
- All-caps: title and section slides only; avoid all-caps body text.
- `calc(-1 * clamp(...))` for negated lengths instead of `-clamp(...)`.

### Color
- Accent: **one focal element per slide** — main heading OR one highlighted figure, not both.
- Max 3 distinct colors visible per slide: bg, text, accent. Muted is a bg derivative.
- Dark themes: WCAG AA (4.5:1) minimum. Validate new accents before shipping.
- Gold/yellow accents only on dark backgrounds — they fail contrast on light.

### Layout
- Slide padding: `var(--pad)` on all sides; usually not tighter than `var(--sp-6)`.
- **Canonical regions (a slide uses up to four):**
  - `.slide-logo` — optional, top-right brand mark
  - `.slide-header` — optional, holds `.title` + `.description`
  - `.slide-content` — required, smart flex body
  - `.slide-footer` — optional, source / page / link
  Only `.slide-content` is required. Logo and footer are usually deck-wide for visual rhythm, but can be skipped on hero / section slides — pick a posture and stick to it. The skeleton is a contract for *where* things sit when present, not a prescription that every slide must show every region.
- Centered slide types (`title`, `section`, `quote`, `closing`) center their stack vertically; `title` and `closing` also center horizontally.
- Per-slide flex / grid wireframes and the content-signal → layout decision table live in `references/wireframes.md`.

### Slide Density (hard limits)

| Type | What it delivers | Max content |
|------|-----------------|-------------|
| `title` | First impression, tone | Title + subtitle + meta (3 lines) |
| `agenda` | Roadmap — what's coming and why | 4–6 items, short labels |
| `section` | Topic transition, mental reset | Label + 1-line teaser |
| `content` | Claim + supporting evidence | Heading + max 4 bullets (12 words each) |
| `two-col` | Comparison, before/after, two views | Heading + 2 cols × 2–3 bullets |
| `stats` | A single number that IS the point | 1–2 large numbers + label + 1-line context |
| `quote` | External credibility, emotional punch | 1 quote ≤ 30 words + attribution |
| `code` | Working proof, concrete example | 1 code block ≤ 20 lines + title claim |
| `chart` | Data-driven proof, trend, distribution | 3–5 data points; title states the finding |
| `image` | Visual proof, atmosphere, diagram | 1 image + title + optional 1-line caption |
| `timeline` | Sequence, journey, roadmap | 3–5 steps with short labels |
| `comparison` | Decision matrix, option analysis | 2 cols × 3 rows max |
| `closing` | Land the conclusion, drive action | Headline + CTA + next step |

---

## Anti-Slop Guide

### Explicitly banned — avoid in any deck

- Inter or Roboto as the **only** heading font (fine as body; banned as sole head)
- `background-clip: text` gradient fade on headings
- Purple `#7c3aed` / `#8b5cf6` or cyan `#06b6d4` + fuchsia as accent (neon dashboard cliché)
- Multiple radial gradients blended as a background ("gradient mesh blob")
- Animated glowing `box-shadow` on cards
- Emoji leading every bullet or section heading
- Three-dot window chrome (`• • •`) on every code block
- Identical layout on every slide — centered stack repeated 15 times
- More than 3 accent-colored elements per slide

### The swap test

After generating the deck: if you swapped the title with a competitor's name and nothing else felt different, the design likely failed. Aim for **at least one visual decision that couldn't come from a default template**.

---

## Resources

### Fonts

**Google Fonts** — `https://fonts.google.com`
Confirmed distinctive options: Fraunces, Space Grotesk, Bricolage Grotesque, Outfit, Instrument Serif, Sora, Playfair Display, Plus Jakarta Sans, Cabinet Grotesk.
Note: Inter is available, but use it as the sole heading font only when the brand or audience calls for a very neutral voice.

**Fontshare** — `https://www.fontshare.com`
Free for commercial use. Strong display options: Cabinet Grotesk, General Sans, Sentient.

**IBM Plex** — `https://github.com/IBM/plex`
Cohesive family (Sans, Serif, Mono). Available on Google Fonts.

---

### Color & Accessibility

**WebAIM Contrast Checker** — `https://webaim.org/resources/contrastchecker`
Validate every `--text` / `--bg` and `--accent` / `--bg` pair. Must reach 4.5:1.

**Coolors** — `https://coolors.co`
Palette generator with OKLCH export. Useful for building surface/border/muted from a base background.

**Reasonable Colors** — `https://www.reasonable.work/colors`
Accessible color pairs that pass WCAG AA in all listed combinations.

**Material Design 3 Color** — `https://m3.material.io/styles/color/overview`
Role-based token vocabulary maps directly to the `--bg / --surface / --accent` contract here.

---

### Design Inspiration

**Dribbble** — `https://dribbble.com`
Filter by "presentation" for headline treatment, data visualization, and section divider patterns.

**Behance** — `https://behance.net`
Filter "presentation" by Most Appreciated for real end-to-end deck compositions.

**Slideworks** — `https://slideworks.io/resources/47-real-mckinsey-presentations`
Real consulting firm decks. Best reference for information-dense layouts and the takeaway-title approach.

---

### HTML Presentation Frameworks (for research and inspiration)

**Reveal.js** — `https://revealjs.com` · `https://github.com/hakimel/reveal.js` (68k ⭐)
Layered CSS theming, `data-auto-animate`, speaker notes. Inspect their themes for layout/spacing ideas.

**Slidev** — `https://sli.dev` · `https://github.com/slidevjs/slidev` (34k ⭐)
Markdown + Vue 3. Strong two-column layouts and code annotation patterns.

---

### Design Methodology

**Extended Frames — Visual Hierarchy** — `https://extendedframes.com/visual-hierarchy-in-design-and-presentation`
F/Z eye-path patterns, per-slide-type layout archetypes, 10-point pre-delivery checklist.

**Chronicle — Visual Presentation Principles** — `https://chroniclehq.com/blog/visual-presentations-tips-types-design-principles`
Layout, hierarchy levers, consistency, and the most common failure modes.

---

## PDF Export (Optional)

The `@media print` rule in `base.css` outputs 1280×720 pages. To export with headless Chrome:

```bash
# After: npx serve .octocode/slides/{{slideName}}
chromium-browser --headless --print-to-pdf=output.pdf \
  --no-pdf-header-footer --window-size=1280,720 \
  http://localhost:3000
```
