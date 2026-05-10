# Phase 4 — Design

**Role:** Visual designer. You create a fitting visual identity for this specific deck through deliberate choices grounded in the brief. Be distinctive where it matters, but do not create design process overhead when the goal is speed.

**Input:** `.content/request.md` · `.content/outline.md`
**Output:** `.content/DESIGN.md` · `css/base.css` · `css/theme.css`

> **Design gate (default: ask).** Always show the user 3 design directions and wait for a choice before writing CSS. Skip only when the user explicitly delegates ("fast mode", "your call", "just build it") or a brand guide is locked. The Design Reasoning Chain (Step 2b) must be completed before any aesthetic choice is made.

---

## Step 1 · Read design references

Read now (in parallel):
- `references/design-system.md` — CSS variable contract, design process, anti-slop guide, resources
- `references/resources.md` — CDN libraries, font catalogs, color tools, inspiration sources
- `references/slide-rules.md` §§2–3 — Visual/Design rules and Layout rules (required by Global Rule 9)
- `.content/outline.md` — slide list with inline notes from Phase 3; use Slide notes for special design treatment per slide

---

## Step 2 · Map images from the brief

Read `request.md` → Images section. For every image listed:

| Decision | What to record |
|----------|----------------|
| Is the image file ready? | `ready` → use `<img src="{{path}}">` directly in Phase 5 |
| Is the image "user will provide"? | `placeholder` → use the `image-ph` / `image-ph-bleed` pattern from `references/html-templates.md` |
| Full-bleed or inline? | Full-bleed → `slide--image` type; inline → `image-ph` inside `content` or `two-col` |
| Does image require text overlay? | Add `image-overlay` gradient div when needed to keep text legible on full-bleed slides |

Record as a table in `DESIGN.md` → Layout notes. Also update each affected slide's inline notes in `outline.md` (`Images` and `UX / UI`). Leave placeholder slides marked with `[IMAGE PLACEHOLDER]` in the outline so Phase 5 knows to use the placeholder template.

If no images were listed in the brief: skip this step.

---

## Design Reasoning Chain — complete this before any aesthetic decision

Every design choice must trace back to the audience and goal. Work through these six questions in order. Write 1–2 sentence answers for each in `DESIGN.md → Visual identity`. Never skip this chain to jump to "looks cool."

| Step | Question | What a strong answer sounds like |
|------|----------|----------------------------------|
| 1 · Audience signal | Who is in the room and what do they trust? | "Developers who distrust vague claims → high contrast, code-native fonts, specific numbers." |
| 2 · Energy | Dark (focused, projector) or Light (readable, print)? | "Stage / conference room → dark background dominant." |
| 3 · Temperature | Warm, cool, or neutral? | "AI/infrastructure topic → cool (blues, slates) signals precision." |
| 4 · Personality | What emotion should the first slide trigger? | "Trust and momentum — not excitement, not fear." |
| 5 · Differentiation | What does the competition look like, and what should this NOT look like? | "Generic startup decks use cyan+magenta on black → avoid, use slate+amber instead." |
| 6 · Constraint | Is there a brand guide, color palette, or font stack locked by the user? | "No guide provided → full design latitude." |

**Only after completing this chain:** pick colors, pick fonts, pick layouts, pick libraries. Every pick should have a one-line reason that points back to a step above.

---

## Step 3 · Analyze the deck's design context

Before searching for inspiration, answer these questions from the brief and outline:

| Question | What it drives |
|----------|----------------|
| What emotion should the first slide trigger? | Color energy and contrast level |
| Is the content data-heavy, narrative, or code-driven? | Layout density and type personality |
| Formal boardroom, tech conference, or creative studio? | Restraint vs. expression |
| Does the deck live in dark environments (projector, stage)? | Dark vs. light background |
| What does the competition / industry look like? | What to differentiate from |
| Are there user-provided images that anchor the visual style? | Palette should complement image tones |

Write your analysis — 5–6 short answers. This drives your research direction.

---

## Step 3b · Review per-slide design needs

Before writing CSS, check `outline.md → Slide notes` for any slide whose visual treatment is not obvious.

For each slide with special treatment in `Slide notes`, confirm:

| Section | What Phase 4 should add or confirm |
|---------|----------------------------------|
| `Content` | Final on-slide text is short enough for the chosen layout; flag overloaded copy before implementation |
| `Data` | Exact values, units, source, and intended visual encoding for any metric or chart |
| `Widgets` | Component choice and library, e.g. Motion counter, code block, Mermaid diagram, callout, progress bar |
| `Graphs` | Chart type, library, data mapping, labels, key insight, source |
| `Images` | File path or placeholder, alt text, crop/framing, overlay, caption |
| `UX / UI` | Layout type, dominant visual, reading order, density, animation, accessibility/contrast |

If a design review reveals a slide is overloaded, add a split note in `outline.md → Slide notes` and flag it for Phase 5.

---

## Step 4 · Research visual direction (skip if brand_guide: locked)

Do this when the brief asks for a custom aesthetic, the topic benefits from visual research, or the direction is not obvious. Skip deep visual research when a brand guide is locked, the user delegated choices, or the design-system matrix clearly matches the goal.

When research is useful, run at least 2 of these in parallel based on your analysis and the tools available in the current agent environment:

```
Open or search Dribbble for "{{mood}} presentation"
Open or search Behance for "{{context}} presentation"
Search GitHub/Octocode for "HTML presentation CSS theme {{aesthetic}}"
Open or search Awwwards for "{{aesthetic}}"
Open Fontshare for distinctive display fonts
```

Extract: color pairings, font personalities, layout patterns, spacing rhythms.
Take notes — don't copy. The goal is direction, not theft.

For fonts: pick a heading/body pair from `design-system.md` → Font Pairing Presets, OR find something new via Google Fonts / Fontshare. Guideline: the heading font should have enough personality to feel chosen, not defaulted.

Fallback if these tools are unavailable: use available web search/browser tools, official font catalogs, local examples, or the curated palettes and font pairings in `design-system.md`.

---

## Step 5 · Generate style previews — MANDATORY unless fast mode

**Default: always generate previews and ask the user to choose before writing CSS.**

Design is subjective. An agent that silently auto-selects colors and fonts will frequently produce a deck the user hates — then Phase 5 has to be redone. Spend 3 minutes on previews, not 30 minutes on a wrong theme.

**Exceptions — skip previews and auto-select only when:**
- User explicitly said "fast mode", "your call", "just build it", or "skip design"
- A `brand_guide: locked` entry exists in `request.md`
- The user already approved a specific theme in this conversation

In all other cases, generate three previews.

When previews are needed, write exactly three standalone HTML files — each a different visual direction:
- `.content/preview-a.html`
- `.content/preview-b.html`
- `.content/preview-c.html`

Each preview shows **the title slide only**, fully rendered: color palette, font pair, heading hierarchy, spacing, one accent element. Must look great when opened in a browser.

**Rules for the three previews:**
- Each should have a **distinct visual direction** — not just the same layout with a different accent color
- Prefer a mix of dark and light backgrounds when that helps the user choose
- None may copy color values verbatim from `design-system.md` themes
- None may score 2+ on the Visual Slop Test, and none may fail the Content Slop Test (from SKILL.md)

Show the user:

```
Three style directions ready:

  A → .content/preview-a.html  ({{8-word description of the aesthetic}})
  B → .content/preview-b.html  ({{8-word description}})
  C → .content/preview-c.html  ({{8-word description}})

Open them and pick one — or describe what to change.
```

**Gate 4a — Smart stop.** Stop for a choice only when previews were generated. Write DESIGN.md after the user picks a direction or delegates the choice.

**If the user rejects all three previews:** Ask for one concrete direction change ("darker", "more minimal", "warmer colors", "different font personality") and generate one revised preview incorporating that direction. Repeat Gate 4a. Generate 3 new previews only when the user explicitly requests it.

**If `brief.md` has `brand_guide: locked`:** SKIP Steps 4–5 (research and style previews). Still do Step 2 (image mapping) and Step 3 (context analysis). Read the brand values from the brief, map them directly to DESIGN.md tokens, and proceed to Step 6.

---

## Step 6 · Write DESIGN.md

Write `.content/DESIGN.md` inside `.octocode/slides/{{slideName}}/`. Keep it short and actionable. Every decision should explain the WHY, not just the WHAT.

```markdown
# DESIGN.md — {{Deck Title}}

> Visual system for this presentation. All CSS values come from this document.

## Visual identity

**Mood:** {{Two sentences — how it feels and what it communicates to this audience}}
**Inspiration:** {{What informed the choices — describe the source, include URLs if from research}}
**Distinctive choice:** {{The one design decision that sets this deck apart from a generic template}}

## Color system

| Token | Value | Role |
|-------|-------|------|
| `--bg` | `{{hex or oklch}}` | Slide background |
| `--surface` | `{{}}` | Card / panel backgrounds |
| `--border` | `{{}}` | Dividers, code block edges |
| `--accent` | `{{}}` | One focal element per slide |
| `--text` | `{{}}` | Body text |
| `--muted` | `{{}}` | Captions, metadata, labels |
| `--code-bg` | `{{}}` | Code block background |
| `--code-text` | `{{}}` | Code block text |

Contrast: `--text` vs `--bg` = {{ratio}} · Must be ≥ 4.5:1 WCAG AA

## Typography

| Token | Font | Weight | Use |
|-------|------|--------|-----|
| `--font-head` | {{Google/Fontshare name}} | 700 | Slide headings |
| `--font-body` | {{Google/Fontshare name}} | 400/500 | Body text, bullets |
| `--font-mono` | {{Font}} | 400 | Code blocks |

Google Fonts `@import` URL: `{{full URL}}`

Type scale (all clamp — no raw px or rem on text):
- `--t-display`: `{{clamp(Xrem, Yvw, Zrem)}}` — title slides only
- `--t-title`:   `{{clamp(...)}}` — slide headings
- `--t-sub`:     `{{clamp(...)}}` — subtitles, column headings
- `--t-body`:    `{{clamp(...)}}` — bullets, paragraphs
- `--t-small`:   `{{clamp(...)}}` — captions, metadata

## Layout notes

List only slides that need special design attention. All slide-level notes live in `.content/outline.md → Slide notes`.

| Slide | Special treatment |
|-------|------------------|
| {{01-title}} | {{Full-bleed? Background image? Specific layout?}} |
| {{NN-chart}} | {{Chart.js? SVG? Color mapping?}} |

## Animation approach

- Simple entrances: CSS `.fade-in` / `.slide-up` from `base.css`
- Sequences / stagger / counters: Motion (motion.dev) on slides: {{list or "none"}}
- `@media (prefers-reduced-motion: reduce)` respected in every animated slide

## Libraries

Only list libraries actually needed. For each: which slide, and why this library over alternatives.

| Library | Slides | Why this library |
|---------|--------|------------------|
| {{highlight.js}} | {{05}} | Code slide — real syntax colours, not faked |
| {{Chart.js}} | {{06}} | Bar/line/donut — lightest option |
| {{Motion}} | {{01}} | Counter/stagger — CSS alone cannot sequence |

**Library decision rules (one chart lib per slide):**
- Bar, line, area, donut, scatter → **Chart.js** (default, lightest)
- Heatmap, geo, treemap, candlestick → **ECharts**
- Dense time-series 100+ points → **uPlot**
- Polished look, minimal config → **ApexCharts**
- Custom bespoke / network → **D3.js**
- Number counter / spring animation → **Motion** (not a chart lib)
- Flow / sequence diagram → **Mermaid** (not a chart lib)
- Static comparison ≤6 bars → **CSS only**, no library
```---

## Step 7 · Generate CSS files

1. `css/base.css` at the deck root — copy the boilerplate from `references/html-templates.md` verbatim
2. `css/theme.css` at the deck root — Google Fonts `@import` at top, then override every CSS variable using exact values from DESIGN.md

---

## Gate 4b — Smart stop

Show this only when user design approval is needed. In fast/delegated mode, send a short update and continue to Phase 5.

```
DESIGN.md written. Theme locked.

Key choices:
- Color: {{accent is X, chosen because Y}}
- Font: {{X for headings (why it fits), Y for body}}
- Distinctive: {{what makes this deck visually unique}}

Libraries: {{list or "none"}}

Review DESIGN.md at .content/DESIGN.md, then reply "good" to start building slides,
or describe any changes.
```

Stop only when approval is required. After approval, or when continuing in delegated mode, delete previews if they exist: `rm .content/preview-*.html`
