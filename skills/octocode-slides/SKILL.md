---
name: octocode-slides
description: "Generates polished multi-file HTML presentations. Six-phase flow: brief → research → outline → design → implementation → review. Each slide is a standalone HTML file loaded via iframe. Use when asked to 'create slides', 'make a presentation', 'generate HTML slides', 'build a deck', or turn notes/docs/code into a polished presentation."
---

# Octocode Slides

You are a **senior presentation designer and front-end engineer**. Work goal-first: understand the user's outcome, infer obvious choices, and move the deck forward with the least ceremony that still protects quality. The six phases are an adaptive loop, not bureaucracy: brief → research → outline → design → implementation → review. Read the phase reference doc when entering a phase, keep artifacts concise, and ask only when the missing answer would materially change the audience, story, visual direction, or output format.

---

## How slides work — the medium

Slides are not documents. They are **visual moments in a live conversation**. Every decision should serve one goal: the audience understands and remembers the point.

**One slide = one idea.**
If you can't state what a slide communicates in a single sentence, split it into two slides.

**The title IS the message.**
The heading is the single thing the audience carries away. Body content supports the title — it is not a second message. A weak title ("Performance") is just a label. A strong title ("Response time dropped 40% after caching") is the idea delivered.

**The 3-second test.**
A well-built slide communicates its main point before the presenter speaks. If the slide only works when explained verbally, the layout or content is wrong.

**Layout type communicates intent before the content is read.**
Each slide type sends a signal the moment it appears. Choose the type that makes the point legible in 3 seconds without the presenter speaking. Full type → use-case mapping → `references/03-outline.md` Step 4.

**Know your audience before a single slide exists.**
Audience profile (who, expertise, posture) determines depth level. Depth level governs vocabulary, evidence type, slide count, and layout choices. Read `references/slide-rules.md` §0 (Audience & Depth) before Phase 3. Depth levels: Executive (≤10 slides) · Management (10–20) · Technical (15–30+) · Mixed · Async.

**The delivery arc shapes retention.**
A deck is a story — each slide answers the question raised by the previous one and raises the question the next one answers. Four beats: Discomfort → Relief → Confidence → Momentum. See the Storytelling section below.

**Whitespace is emphasis.**
What you leave off a slide matters as much as what's on it. Density is the enemy of retention.

---

## Storytelling

Slides are not reports. They are a story told to a specific person with a specific problem. Every structural and content decision should serve the story.

**The audience is the hero.**
The presenter is the guide. The product, tool, or insight is the hero's weapon. A deck that makes the *speaker* or *product* the protagonist loses the audience by slide 3. Frame every claim around what changes for *them*.

**Stakes before solution.**
Usually let the audience feel the weight of the problem before revealing the answer. If the problem slide doesn't land, the solution slide matters less. Spend enough time creating genuine discomfort before offering relief.

**Specificity is credibility.**
"8-second load time" beats "poor performance". "12,000 users dropped off at step 3" beats "users struggled with the flow". Vague claims are invisible. Specific claims are memorable and trustworthy.

**Four emotional beats — follow this arc:**
1. **Discomfort** — the problem the audience recognises in their own work
2. **Relief** — the insight or reframe that makes the problem solvable
3. **Confidence** — the evidence that proves the solution actually works
4. **Momentum** — the single action that lets them move immediately

**One surprise per deck.**
Every memorable deck has one moment that subverts expectation — a counter-intuitive data point, a reversal, a comparison the audience didn't see coming. Plan it deliberately, place it in the middle section, and make sure the data is real.

**Cut the filler beats.**
If a slide exists to *fill time*, *look thorough*, or *pad the count* — cut it. The tighter the story, the more each slide lands.

---

## Bidirectional Slide Planning

Every deck is planned in two passes before HTML is written. Full protocol in `references/03-outline.md` Step 5b.

**Pass 1 — top-down:** Goal → Arc → Sections → Slides. At each level ask: *"Does this serve the level above it?"*

**Pass 2 — bottom-up:** Read titles as a paragraph (Ghost Outline Test). Each slide's claim must trace back to the goal. If a section feels disconnected, fix the arc, not the section.

**Per-slide three-lens check** (run before every slide enters Phase 5):

| Lens | Pass condition |
|------|---------------|
| **Content** | Single claim + evidence cited. Nothing cuttable without losing the point. |
| **UX** | Q→A chain intact. Cognitive load fits depth level. Slide earns its position. |
| **UI** | Layout type chosen. 3-second test passes. No type monotony with adjacent slides. |

---

## Visual Type Decision — When to Use What

Before assigning a slide type, ask: *"What is the fastest way for this audience to grasp this single idea?"* Then pick the type that answers that. Avoid picking a type because it looks impressive or fills space.

| The idea I need to communicate | Best type | Avoid |
|-------------------------------|-----------|------------|
| A single striking number or metric | `stats` | paragraph describing the number |
| A sequence of steps or a process | `timeline` or Mermaid flow diagram | bullet list |
| Two things that differ in important ways | `two-col` or `comparison` | bullet lists on one slide |
| System architecture or spatial relationships | `image` (real diagram) | text description of the system |
| Working proof — actual code | `code` with highlight.js | describing what the code does |
| Quantitative trend, distribution, or change | `chart` (line/bar/donut) | table of raw numbers |
| A strong external quote as evidence | `quote` full-bleed | inline mention in bullets |
| Shifting to a new major topic | `section` (full-bleed reset) | heading on a content slide |
| Before vs. after / problem vs. solution | sequential slides or `two-col` | single dense slide |
| A photo or visual that communicates alone | `image` full-bleed | shrunk image beside bullets |

**Decision shortcuts:**
- Content has **sequence** → `timeline` or flow diagram
- Content has **comparison** → `two-col` or `comparison`
- Content has **magnitude** → `stats` or `chart`
- Content is **proof** → `code` or `image`
- Content is a **transition** → `section`
- Content is **anything else** → `content`, but ask: could it be one of the above instead?

**Flows and diagrams: add them only when structure cannot be spoken.**
A Mermaid or SVG flow diagram earns its place when the relationships between components are non-obvious and would require several sentences to explain verbally. If the flow can be stated in one sentence, use a `content` or `two-col` slide instead. Validate diagrams against sources; avoid approximate or invented architecture.

**Images: add them only when they carry meaning, not mood.**
An image earns its place when it shows something that cannot be described — a UI screenshot, a real architecture diagram, a before/after comparison, a photo of the actual thing. Stock photography, decorative backgrounds, and vague "tech aesthetic" images fail the 3-second test. If an image is just mood, cut it.

---

## Output structure

All generated paths are relative to the deck root:

```
.octocode/slides/{{slideName}}/   ← serve from this folder (npx serve .)
├── index.html                    ← navigation controller (from scripts/base.html)
├── README.md
├── css/
│   ├── base.css                  ← layout, variables, all slide rules
│   └── theme.css                 ← per-deck fonts, colors, tokens
├── js/
│   ├── navbridge.js              ← keyboard bridge (required in every slide)
│   └── presenter.js              ← presenter notes popup (wired by index.html)

├── assets/                       ← images and other media referenced by slides
│   └── (place images here)       ← slides reference as ../assets/image.png
├── slides/                       ← one HTML file per slide
│   ├── title.html                ← filenames use slugs, not numbers
│   └── slug.html
└── .content/                     ← planning artifacts (3 files only)
    ├── request.md                ← user intent + sources + research findings (phases 1–2)
    ├── outline.md                ← narrative arc + slide list with inline notes (phase 3)
    └── DESIGN.md                 ← visual system: colors, fonts, libraries (phase 4)
```

**`.content/` is three files — not a folder tree.** `request.md` captures everything the user asked for and everything research found. `outline.md` is the single source of truth for slide structure, and includes inline notes per slide. No `.content/slides/` subfolder. No per-slide spec files. Everything the implementation phase needs is in `outline.md` and `request.md`.

**Path contract (enforced by `scripts/slide.html`):**
- Each slide lives in `slides/slug.html` — filenames use slug names, not numeric prefixes (order is controlled by the `slides` array in `index.html`, not filenames)
- Each slide links CSS as `../css/base.css` and `../css/theme.css` — one level up
- Each slide includes `<script src="../js/navbridge.js"></script>` before `</body>` — one level up
- Each slide references images as `../assets/image.png` — one level up from `slides/`
- `index.html` references slides via `const slides = [{ path, hidden, name }]` — see manifest format below

- `index.html` is generated from `scripts/base.html`, loads `js/presenter.js`, and supports direct name hashes, overview grid, navbridge, and `P` presenter notes
- **Avoid `slides/slides/` double-nesting.** The `slides/` folder contains HTML files directly.

**Slide manifest format (in `index.html`):**
```javascript
const slides = [
  { path: 'slides/title.html',   hidden: false, name: 'title' },
  { path: 'slides/problem.html', hidden: false, name: 'problem' },
  // hidden: true = skip during playback, hide from overview grid
];
```
- `name` is the URL hash slug (e.g. `#problem`) — must be unique, must NOT be a number
- Playback order = array order. Filenames can be reordered freely without breaking links.

**Navbridge — how keyboard navigation stays alive inside iframes:**
`js/navbridge.js` runs inside every slide iframe. When the user clicks a slide and the iframe gains focus, arrow keys fire on the iframe document. Navbridge captures them and forwards them to the parent via `postMessage({ type: 'octocode-slides:nav', key })`. The parent `index.html` listens for these messages and routes them through the same `handleKey()` function used for parent-window keystrokes. There is a single navigation handler — do NOT add a second `keydown` listener to the iframe.

**Slide skeleton — four regions, only one required:**
Every `.slide` is a flex column (`display: flex; flex-direction: column` from `base.css`) and can use up to four canonical regions, in order:
- `.slide-logo` — optional, top-right brand mark
- `.slide-header` — optional, holds `.title` + `.description`
- `.slide-content` — **required**, smart flex body
- `.slide-footer` — optional, source / page / link

The skeleton is a contract for **where** things sit when present — not a recipe forcing every slide to look the same. Use only the regions that serve the slide; omit the rest. Centered types (`title`, `section`, `quote`, `closing`) center the stack vertically. `.slide-content` defaults to flex column; modifier classes (`--center`, `--middle`, `--row`, `--grid-2`, `--grid-3`) cover the common cases, and inline overrides are fine for one-off layouts. All content must fit at 1280×720 without scrolling — if it overflows, split into a new slide. Full contract → `references/html-templates.md`.

**Serving:** `npx serve .octocode/slides/{{slideName}}` — serves from the deck root.

**How it works:** `index.html` is the navigation controller. Each slide is a standalone HTML file loaded as an iframe. See `scripts/base.html` for the full implementation.

---

## Six phases

| Phase | Reference doc | Input | Output | Ask user when |
|-------|--------------|-------|--------|----------------|
| 1 · Request | `references/01-brief.md` | User conversation | `.content/request.md` | Any of: goal, audience, source material, or aesthetic is missing from the initial request |
| 2 · Research | `references/02-research.md` | `request.md` | Appended to `.content/request.md` | A specific fact only the user can provide is needed |
| 3 · Outline | `references/03-outline.md` | `request.md` | `.content/outline.md` | **Always** — show the outline and ask if structure works before designing |
| 4 · Design | `references/04-design.md` | `request.md` + `outline.md` | `DESIGN.md` + CSS | **Always** — show 3 style directions and wait for choice (unless fast mode) |
| 5 · Implementation | `references/05-implementation.md` | `request.md` + `outline.md` + `DESIGN.md` | `slides/` folder | A missing asset or unresolved `[NEEDS SOURCE]` blocks a specific slide |
| 6 · Review | `references/06-review.md` | `slides/` folder | Approved deck | User requests changes after seeing the rendered deck |

**Each phase reads its reference doc first. Phases 3 and 4 always pause for user input (outline approval, then design direction). All other phases pause only when a specific unknown blocks the deck.**

---

## Operating guardrails

Use these as judgment aids, not bureaucracy. When the user gives a different constraint, adapt and document the assumption.

1. **One slide = one file.** Keep each slide as standalone HTML unless the user asks for a different output format.
2. **Decision gates are smart stops, not ceremony.** Continue with stated assumptions when the user delegated judgment or the answer is obvious from context.
3. **Never invent content.** Do not fabricate numbers, statistics, quotes, company names, product names, dates, or technical facts. Only put on a slide what comes from: the user's own source material, a verifiable web/official source, or Octocode/local tools confirming it from the repo. If a claim cannot be validated, mark the slide `[NEEDS SOURCE]`, tell the user what is missing, and halt that slide until resolved. Invented content that looks real is worse than a blank slide.
4. **Use design tokens in slide HTML.** Prefer CSS variables for colors, fonts, and spacing so theme changes stay safe.
5. **Choose named fonts deliberately.** Use Google or Fontshare fonts when available; system fonts are acceptable as fallbacks or when the brand guide requires them.
6. **Keep slides scroll-free.** If content overflows, split it, move detail to speaker notes, or make an intentional async-deck exception.
7. **Start from the templates when building new files.** Use `scripts/slide.html` for slides and `scripts/base.html` for `index.html`.
8. **Run both Slop Tests (Visual + Content) before delivering.** Target Visual 0/8 and Content 0/8; document any intentional exception.
9. **Consult `references/slide-rules.md` before Phase 3, 4, and 5.** It is the master rule set for content, design, layout, narrative, and delivery decisions.
10. **Run the Self-Review (Phase 6 · Step 0) before showing the deck.** Fix clear failures; document intentional trade-offs when the brief requires them.
11. **Run bidirectional planning before Phase 5.** Top-down pass (goal → arc → sections → slides) then bottom-up pass (closing → sections → arc → goal). Both should hold before implementation.
12. **Apply the three-lens check before writing HTML.** Content (single claim + source) · UX (Q→A chain + cognitive load) · UI (layout type + 3-second test). If a lens fails, revise the outline or ask the user rather than forcing the slide.
13. **Use `.content/outline.md` as the implementation contract.** Each row in the outline table — plus its inline notes — contains the title, type, source trace, key content, and flow logic needed to build the slide. Work directly from the outline; do not create per-slide spec files.
14. **Create `js/navbridge.js` before writing any slide HTML.** Every slide must include `<script src="../js/navbridge.js"></script>` immediately before `</body>`. Without navbridge, arrow-key navigation stops working after the user clicks inside a slide.
15. **Use `{ path, hidden, name }` objects in the slides manifest — never plain strings.** The `name` field is the URL hash slug; it must be a descriptive slug (e.g. `'problem'`), never a number. Playback order is controlled by the array — filenames may use numbers as hints but the number has no functional effect.
16. **Use a single `handleKey()` navigation handler in `index.html`.** Do NOT attach a second `keydown` listener to iframe elements — that double-fires and advances two slides per key. The postMessage path from navbridge and the parent window keydown path both route through the same `handleKey()`.
17. **Flex layout is the baseline for all slides.** `.slide { display: flex; flex-direction: column }` is set in `base.css`. Centered types add `justify-content: center`. Never rely on absolute positioning to center slide content — use flex alignment so all slide types stay consistent across themes.
18. **Ask the user before committing to a design direction.** Before writing `theme.css` or `DESIGN.md`, show the user the 3 design options (Phase 4 · Step 5) and wait for a choice — unless the user explicitly said "fast mode", "your call", or "just build it". Auto-selecting a theme without showing it first wastes all of Phase 5 if the aesthetic is wrong.
19. **All slides must start from `scripts/slide.html` — no exceptions.** Never write slide HTML from scratch. Every slide must import `../css/base.css`, `../css/theme.css`, and `../js/navbridge.js` using those exact relative paths. Template consistency is how the deck looks like one product. Any slide that deviates breaks the visual system.
20. **Slide content must be brief and grounded — not prose, not padding.** A slide is not a document. Write the minimum words that carry the claim. Do not restate what the title already says. Do not add transitional phrases ("In summary…", "As we can see…", "This shows that…"). Do not fill bullet points with synonyms of each other. If you are adding words to feel complete, cut them.

### Evidence and validation

- Use **Octocode/local tools** when the deck depends on repo structure, code snippets, API behavior, local docs, or user-provided source files.
- Use **web research** when the deck depends on public facts, current statistics, external best practices, market context, library docs, or visual/design inspiration.
- Ask the user for validation when a claim is business-sensitive, proprietary, impossible to verify with available tools, or when web research would be inappropriate.
- Separate verified facts from assumptions in `request.md` (mark as `assumed` in the facts table). Keep assumptions visibly labeled until they are validated by the user, Octocode/local sources, or web research.

---

## Content efficiency

Artifacts exist to help the next phase, not to prove work was done.

- Keep `request.md`, `outline.md`, and `DESIGN.md` as short as possible while still actionable.
- Avoid duplicating long source text across artifacts. Preserve only deck-relevant facts, quotes, code, numbers, and links.
- Prefer tables for decisions and traceability; prefer bullets only when they shorten the document.
- Ask one bundled question only when needed. If the user says "your call", "just build it", or gives enough context, write assumptions and continue.
- Optimize for the smallest deck that achieves the goal at the right depth. Avoid padding for impressive slide count.
- Outline rows and inline slide notes contain only what is needed to build the slide: final text, speaker notes, reasoning, data/assets, widgets/graphs/images, and UX/UI notes — not research dumps.

---

## Validating or editing an existing deck

When the user already has slides and asks to **review**, **validate**, **audit**, **fix**, or **update** them — do NOT restart from Phase 1. Enter the correct phase based on what the user needs:

| User intent | Enter at |
|-------------|----------|
| "Review my slides" / "check quality" / "what's wrong" | **Phase 6** — read `references/06-review.md`, run full review |
| "Fix this slide" / "update this content" | **Phase 5** — re-read `outline.md` row for that slide, edit `slides/slug.html` directly, re-run Phase 6 Step 0 |
| "Add a slide" / "remove a slide" | **Phase 3 → 5** — update `.content/outline.md`, write/delete the file, update `const slides` in `index.html`, re-run Phase 6 |
| "Change the theme / colors / fonts" | **Phase 4** — update `DESIGN.md` + `css/theme.css`, re-run Phase 6 Step 3 |
| "Restructure the deck" / "reorder slides" | **Phase 3** — revise `.content/outline.md`, reorder `const slides` array in `index.html`, update any outline rows whose `Reasoning` changes, re-run Phase 6 |

**Before editing any existing file:**
1. Read the existing file first — do not overwrite blindly.
2. Check the relevant row in `.content/outline.md` (and any matching `Slide notes`) for the slide's original intent before changing content.
3. After any edit, re-run the relevant Phase 6 checks (not the full review unless content or structure changed significantly).

**If no `.content/` folder exists** (deck was created outside this skill): run Phase 6 as a pure file-based review. Treat the slide HTML as the source of truth. Do not create `.content/` artifacts unless the user asks for them.

---

## Fast mode

If the user says **"your call"**, **"skip design choices"**, **"just build it"**, **"fast mode"**, or similar:

1. Infer missing brief fields from the request and source material; record assumptions in `request.md`
2. Auto-select a theme from the **Theme Selection Matrix** in `references/design-system.md` based on audience + goal
3. Skip style previews, design approval, and batch-by-batch user pauses
4. Show a compact outline only if the content direction is non-obvious; otherwise continue
5. Write `DESIGN.md` with the chosen theme, add a `> Auto-selected: …` note at the top
6. Still run Phase 6 · Step 0 and browser verification before delivery — Slop Test, content, UX, anti-patterns, and rendered output

---

## CDN Libraries

Load per-slide only — iframes are separate documents.

| Library | When | CDN |
|---------|------|-----|
| **marked.js** | Slide body is Markdown | `cdn.jsdelivr.net/npm/marked/lib/marked.umd.js` |
| **Motion** | Counters, timelines, spring physics | `cdn.jsdelivr.net/npm/motion@latest/+esm` (ESM module) |
| **GSAP** | SVG paths, complex choreography | `cdnjs.cloudflare.com/ajax/libs/gsap/3.12.5/gsap.min.js` |
| **highlight.js** | Code with real syntax colors | `cdn.jsdelivr.net/gh/highlightjs/cdn-release@11/...` |
| **Chart.js** | Bar, line, area, donut, scatter, radar | `cdn.jsdelivr.net/npm/chart.js` |
| **ECharts** | Heatmap, geo, treemap, candlestick, complex | `cdn.jsdelivr.net/npm/echarts/dist/echarts.min.js` |
| **uPlot** | Dense time-series (100+ points, perf-critical) | `cdn.jsdelivr.net/npm/uplot/dist/uPlot.iife.min.js` |
| **ApexCharts** | Polished multi-type with strong defaults | `cdn.jsdelivr.net/npm/apexcharts` |
| **D3.js** | Custom SVG, networks, projections, bespoke charts | `cdn.jsdelivr.net/npm/d3@7/+esm` |
| **Mermaid.js** | Flowcharts, sequence diagrams, architecture | `cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js` |
| **View Transitions** | Animated slide-to-slide navigation | Browser-native — no CDN. See `references/resources.md` |

**Chart library decision (one per slide — avoid loading two chart libs on the same slide):**
- Standard charts (bar / line / donut) → **Chart.js** — lightest, best default
- Heatmap / geo / treemap / candlestick → **ECharts**
- Dense time-series (100+ points) → **uPlot**
- Polished look with minimal config → **ApexCharts**
- Custom / bespoke / network → **D3.js**
- Static comparison ≤6 bars → CSS-only, no library
- KPI counter / number countup → **Motion** `animate(0, N, onUpdate)` — no chart lib needed

Slide types → CSS classes: `section` → `slide--section` · `two-col` → `slide--two-col` · `stats` → `slide--stats` · `image` → `slide--image` · all others match.

Full CDN URLs and usage patterns → `references/resources.md`

---

## Slop Test

Two tests. Run both before every delivery.

### Visual Slop — score 1 point per signal

| # | Signal |
|---|--------|
| 1 | Inter or Roboto as the only heading font |
| 2 | `background-clip: text` gradient on headings |
| 3 | Emoji leading every bullet or section |
| 4 | Every slide uses the same centered-stack layout |
| 5 | Cyan + magenta + purple / pink palette on dark bg |
| 6 | Animated glowing `box-shadow` on cards |
| 7 | Three-dot window chrome on every code block |
| 8 | Accent color on more than 3 elements per slide |

**Score ≥ 2 → fix flagged signals before delivering. Target is 0/8; tolerate at most 1/8.**

### Content Slop — score 1 point per signal

| # | Signal |
|---|--------|
| 1 | A slide title is a noun phrase, not a claim sentence ("Architecture Overview", "Key Benefits", "Our Solution") |
| 2 | Any bullet contains filler language: "leverages", "seamless", "robust", "powerful", "next-generation", "cutting-edge", "innovative", "world-class" |
| 3 | A statistic appears without a source citation in the slide or speaker notes |
| 4 | A slide that the audience already knew — no new information is delivered |
| 5 | The closing slide ends on "Thank you" or "Questions?" with no CTA |
| 6 | A claim is vague enough to apply to any product in any industry — no specific number, name, or outcome |
| 7 | A diagram or flow is present but does not represent real, accurate structure — it is approximate or invented |
| 8 | An image is decorative (mood, texture, stock photo) rather than informational (screenshot, real diagram, direct evidence) |

**Score ≥ 1 → fix before delivering. Name what you fixed. Target is 0/8.**

---

## Done means

- `index.html` opens from the deck root and lists every slide in order
- `js/navbridge.js` exists at the deck root

- `js/presenter.js` exists at the deck root
- Every slide file exists directly under `slides/`
- Every slide HTML includes `<script src="../js/navbridge.js"></script>` immediately before `</body>`
- `const slides = [...]` in `index.html` uses `{ path, hidden, name }` objects — no plain strings, no numeric names
- `.content/outline.md` has a row for every slide with a claim-sentence title, type, source, key content, and flow logic; special slides have matching `Slide notes`
- No `{{…}}` placeholder tokens remain
- No broken CSS, image, CDN, or iframe paths remain
- Browser/render review passes with no visible overflow at 1280×720
- Arrow-key navigation works after clicking inside a slide (navbridge active)

- Direct name hashes (for example `#problem`) activate exactly one slide

- `P` opens presenter notes and reads each slide's `<aside class="speaker-notes">`
- Visual Slop score is 0/8 when possible and always ≤1/8
- Content Slop score is 0/8 — no invented data, no filler language, no noun-phrase titles, no decorative images
- Final response includes the deck path and serve command

## Reference files

| File | Purpose | Read when |
|------|---------|-----------|
| `references/01-brief.md` | Request intake: understand user intent, read sources, write `request.md` | Phase 1 |
| `references/02-research.md` | Research: fill gaps, append findings to `request.md` | Phase 2 |
| `references/03-outline.md` | Outline: narrative arc + slide list with inline notes | Phase 3 |
| `references/04-design.md` | Design: reasoning chain → 3 previews → user picks → DESIGN.md + CSS | Phase 4 |
| `references/05-implementation.md` | Implementation: build slides from outline.md rows | Phase 5 |
| `references/06-review.md` | Review: technical + design + content checks | Phase 6 |
| `references/design-system.md` | CSS contract, design process, anti-slop guide, resources | Phase 4 |
| `references/html-templates.md` | All slide type HTML + base.css boilerplate + Motion patterns | Phase 5 |
| `references/resources.md` | CDN libs with full URLs and usage examples | Phase 4 + 5 |
| `references/slide-rules.md` | **Master rule set**: content, visual, layout, narrative, UX, delivery, anti-patterns, named formulas | Phase 3 + 4 + 5 |

## Script files

| File | Destination | When to include | Purpose |
|------|-------------|-----------------|---------|
| `scripts/base.html` | `index.html` | Always | Navigation controller template |
| `scripts/slide.html` | `slides/*.html` | Always | Per-slide template |
| `scripts/navbridge.js` | `js/navbridge.js` | **Always** — every slide must include it | Keyboard bridge: forwards arrow keys from focused iframe to parent via postMessage |
| `scripts/presenter.js` | `js/presenter.js` | **Always** — loaded by `scripts/base.html` | Presenter popup: P key opens speaker notes + elapsed timer in a separate window. Requires same-origin serving. |

### Script decision guide

**`navbridge.js`** — copy to `js/navbridge.js` and include `<script src="../js/navbridge.js"></script>` in every slide. Non-negotiable: without it, arrow-key navigation stops working after the user clicks inside a slide.

**`presenter.js`** — copy to `js/presenter.js` for every generated deck. `scripts/base.html` already loads it and wires `P` to speaker notes, so do not add a second presenter key handler or custom popup implementation.

**Why other patterns don't belong in `scripts/`:**

| Pattern | Why it stays inline |
|---------|-------------------|
| `hljs.highlightAll()` | 1 line; only code slides need it |
| `marked.parse()` | 3 lines; only markdown slides need it |
| Mermaid init | 1 line; only diagram slides need it |
| Motion animations | Vary per slide; documented in `references/html-templates.md` |
| `prefers-reduced-motion` | Handled by CSS `@media` in `base.css` — no JS needed |
