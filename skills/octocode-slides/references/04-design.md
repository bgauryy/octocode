# Phase 4 — Design

**Role:** Visual designer. You create a fitting visual identity for this specific deck through deliberate choices grounded in the brief. Be distinctive where it matters, but do not create design process overhead when the goal is speed.

**Input:** `.content/brief.md` · `.content/outline.md` · `.content/research.md` · `.content/slides/*.md`
**Output:** `.content/DESIGN.md` · `css/base.css` · `css/theme.css` · updated `.content/slides/*.md` UX/UI sections

> **Smart gates in this phase:** Ask for visual approval only when the user wants to choose a direction, a brand decision is ambiguous, or the deck is high-stakes enough that subjective design choice should not be inferred. If the user says "fast mode", "your call", or "just build it", auto-select the direction and continue.

---

## Step 1 · Read design references

Read now (in parallel):
- `references/design-system.md` — CSS variable contract, design process, anti-slop guide, resources
- `references/resources.md` — CDN libraries, font catalogs, color tools, inspiration sources
- `references/slide-rules.md` §§2–3 — Visual/Design rules and Layout rules (required by Global Rule 9)
- `.content/slides/*.md` — per-slide specs from Phase 3; use these to design each slide without re-interpreting content

---

## Step 2 · Map images from the brief

Read `brief.md` → Images inventory section. For every image listed:

| Decision | What to record |
|----------|----------------|
| Is the image file ready? | `ready` → use `<img src="{{path}}">` directly in Phase 5 |
| Is the image "user will provide"? | `placeholder` → use the `image-ph` / `image-ph-bleed` pattern from `references/html-templates.md` |
| Full-bleed or inline? | Full-bleed → `slide--image` type; inline → `image-ph` inside `content` or `two-col` |
| Does image require text overlay? | Add `image-overlay` gradient div when needed to keep text legible on full-bleed slides |

Record as a table in `DESIGN.md` → Layout notes. Also update each affected slide spec's `Images` and `UX / UI` sections. Leave placeholder slides marked with `[IMAGE PLACEHOLDER]` in the outline so Phase 5 knows to use the placeholder template.

If no images were listed in the brief: skip this step.

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

## Step 3b · Refine per-slide design specs

Before writing HTML, each slide spec should describe the design plan at slide level. Update `.content/slides/slug.md` for any slide whose visual treatment is not obvious.

For each spec, ensure these sections are actionable:

| Section | What Phase 4 should add or confirm |
|---------|----------------------------------|
| `Content` | Final on-slide text is short enough for the chosen layout; flag overloaded copy before implementation |
| `Data` | Exact values, units, source, and intended visual encoding for any metric or chart |
| `Widgets` | Component choice and library, e.g. Motion counter, code block, Mermaid diagram, callout, progress bar |
| `Graphs` | Chart type, library, data mapping, labels, key insight, source |
| `Images` | File path or placeholder, alt text, crop/framing, overlay, caption |
| `UX / UI` | Layout type, dominant visual, reading order, density, animation, accessibility/contrast |

Avoid changing `Title`, `Description`, or `Reasoning` unless the outline is also updated. If a design choice reveals that a slide is overloaded, split it now and update both `outline.md` and the slide specs.

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

## Step 5 · Generate style previews only when useful

Generate previews when the user asked to choose, the brand/aesthetic is subjective, or you need to de-risk the design before building. Otherwise skip this step, write `DESIGN.md`, and continue.

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

List only slides that need special design attention. Detailed per-slide choices live in `.content/slides/slug.md`.

| Slide | Special treatment |
|-------|------------------|
| {{01-title}} | {{Full-bleed? Background image? Specific layout?}} |
| {{NN-chart}} | {{Chart.js? SVG? Color mapping?}} |

## Animation approach

- Simple entrances: CSS `.fade-in` / `.slide-up` from `base.css`
- Sequences / stagger / counters: Motion (motion.dev) on slides: {{list or "none"}}
- `@media (prefers-reduced-motion: reduce)` respected in every animated slide

## Libraries

| Library | Slides | Why |
|---------|--------|-----|
| {{marked.js}} | {{03, 07}} | Markdown content slides |
| {{highlight.js}} | {{05}} | Code syntax highlighting |
| {{Chart.js / ECharts / uPlot / ApexCharts / D3.js}} | {{06}} | Graph from slide spec |
| {{Motion}} | {{01, 08}} | Entrance stagger / counter |
```

---

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
