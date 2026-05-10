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
Each slide type sends a signal the moment it appears. Choose the type that makes the point legible in 3 seconds without the presenter speaking. Full content-signal → type table lives in `references/slide-rules.md` §4.11; wireframe-level layout selection in `references/wireframes.md`.

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

## Visual Type Decision — quick shortcuts

Pick the type that makes the point legible in 3 seconds. Decision shortcuts:

- Content has **sequence** → `timeline` or flow diagram
- Content has **comparison** → `two-col` or `comparison`
- Content has **magnitude** → `stats` or `chart`
- Content is **proof** → `code` or `image`
- Content is a **transition** → `section`
- Content is **anything else** → `content`, but ask: could it be one of the above instead?

**Diagrams + images earn their place only when structure cannot be spoken / mood is not the point.** Full content-signal → type table, plus diagram and image guidance, lives in `references/slide-rules.md` §4.11. Wireframe-level layout selection is in `references/wireframes.md`.

---

## Output structure

All generated paths are relative to the deck root:

```
.octocode/slides/{{slideName}}/   ← serve from this folder (npx serve .)
├── index.html                    ← navigation controller (from scripts/base.html)
├── README.md
├── css/
│   ├── base.css                  ← copied verbatim from scripts/base.css
│   └── theme.css                 ← per-deck fonts, colors, tokens (overrides only)
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

**`.content/` is three files — not a folder tree.** `request.md` (Phases 1–2), `outline.md` (Phase 3 — single source of truth for slide structure + inline per-slide notes), `DESIGN.md` (Phase 4). No per-slide spec files; no `.content/slides/`.

**Path contract (enforced by `scripts/slide.html` + `scripts/base.html`):**
- Slides live in `slides/slug.html` (no double-nesting) and reference `../css/base.css`, `../css/theme.css`, `../js/navbridge.js`, `../assets/*` (one level up).
- `index.html` references slides via `const slides = [{ path, hidden, name }]` — order is the array, not the filename.
- `index.html` is generated from `scripts/base.html`, loads `js/presenter.js`, and supports direct name hashes, overview grid, navbridge, and `P` presenter notes.

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
| 3 · Outline | `references/03-outline.md` | `request.md` | `.content/outline.md` | Default: pause to confirm structure. Skip in fast mode. |
| 4 · Design | `references/04-design.md` | `request.md` + `outline.md` | `DESIGN.md` + CSS | Default: show 3 style directions and wait. Skip in fast mode or when brand is locked. |
| 5 · Implementation | `references/05-implementation.md` | `request.md` + `outline.md` + `DESIGN.md` | `slides/` folder | A missing asset or unresolved `[NEEDS SOURCE]` blocks a specific slide |
| 6 · Review | `references/06-review.md` | `slides/` folder | Approved deck | User requests changes after seeing the rendered deck |

**Each phase reads its reference doc first.** Phases 3 and 4 are gates because getting structure or aesthetic wrong cascades expensive rework — pausing 30 seconds beats rebuilding 30 slides. Other phases continue until something specific blocks them.

---

## Operating principles

Two layers — both matter, but they're not equal. Hard constraints are structural correctness; if you break one, the deck is broken. Strong defaults are how good decks behave; deviate when the audience, brief, or content makes the override obviously right, and write the reason in `DESIGN.md` or the outline notes.

### Hard constraints — non-negotiable

1. **No fabricated content.** Numbers, quotes, names, dates, code, and architecture must come from user sources, verified web sources, or Octocode/local tools. If a claim can't be validated, mark the slide `[NEEDS SOURCE]` and halt it until resolved. Invented content that looks real is worse than a blank slide.
2. **Path contract.** Slides live in `slides/slug.html` and reference `../css/base.css`, `../css/theme.css`, `../js/navbridge.js`, `../assets/*`. Every slide starts from `scripts/slide.html`. Every slide includes `<script src="../js/navbridge.js"></script>` before `</body>` — without it, arrow-key navigation dies after the user clicks inside a slide.
3. **Single navigation handler.** `index.html` is built from `scripts/base.html` and routes both parent keystrokes and iframe `postMessage` events through one `handleKey()`. Do not add a second `keydown` listener — it double-fires.
4. **Manifest format.** `const slides = [...]` uses `{ path, hidden, name }` objects. `name` is a descriptive slug (`'problem'`, never `'1'` or `'2'`). Array order = playback order; filenames are free.
5. **Outline is the implementation contract.** Build slides from `.content/outline.md` rows. If implementation reveals a better title, split, or order — update the outline first, then build. No per-slide spec files.

### Strong defaults — override with a written reason

- **Design tokens only in slide HTML.** `var(--accent)`, `var(--t-title)`, etc. No raw hex/rem/pixel values. Flex layout is the baseline; absolute centering breaks at theme switches.
- **Named fonts chosen deliberately.** Google or Fontshare fonts beat system fonts unless the brand guide says otherwise.
- **One claim per slide, scroll-free at 1280×720.** If content overflows, split rather than shrink. If you're adding words to feel complete, cut.
- **No filler language.** No "In summary…", "As we can see…", "Key takeaways:". The title carries the claim; bullets support, never restate.
- **Bidirectional planning + three-lens check before HTML.** Top-down (goal → arc → slides) and bottom-up (titles read as a paragraph). Each slide passes Content / UX / UI lenses (defined under "Bidirectional Slide Planning" above).
- **Both Slop Tests pass before delivery.** Visual ≤1/8, Content 0/8. Document any intentional exception.
- **Phase 3 and Phase 4 always pause for user input.** Outline approval, then design direction. Skip only when the user said "fast mode", "your call", "just build it", or a brand guide is locked.
- **Pointer chrome is default-on for live presentations.** A custom cursor + mouse-down spark makes the deck feel like a live console — well suited to talks, demos, and dark/tech themes. Phase 4 confirms or removes it; remove only when the brief calls for print/PDF-first output, the deck is async/silent, or the user opts out. Libraries and wiring → `references/resources.md` → Pointer & Click Feedback.
- **Master rule set is `references/slide-rules.md`.** When this file and a phase doc disagree on a default, the more specific rule wins; record the resolution.

### Evidence

- **Octocode / local tools** for repo structure, code snippets, API behavior, local docs, user files.
- **Web research** for public facts, current stats, external best practices, library docs, visual inspiration.
- **Ask the user** when a claim is proprietary, business-sensitive, or unverifiable with available tools.
- Separate verified facts from assumptions in `request.md` (mark `assumed`). Keep assumptions visibly labeled until validated.

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

## Libraries

Load libraries per-slide only — each iframe is a separate document. Pick the lightest tool that delivers the slide's intent; avoid loading two chart libraries on one slide.

**Single source of truth:** `references/resources.md` — full CDN URLs, decision tables for chart libraries, animation tools, and code rendering. Read it during Phase 4 (when picking libraries for the deck) and Phase 5 (when wiring them into slides).

Slide types → CSS classes: `section` → `slide--section` · `two-col` → `slide--two-col` · `stats` → `slide--stats` · `image` → `slide--image` · all others match.

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

The full handoff checklist lives in `references/06-review.md` Step 2 (technical) and Step 4 (content & flow). High-bar summary:

- Deck serves cleanly at `npx serve .octocode/slides/{{slideName}}` and renders without console errors
- Every slide passes the no-scroll, navbridge-loaded, claim-title, no-`{{…}}` checks
- Manifest in `index.html` uses `{ path, hidden, name }` objects with unique slug names
- Visual Slop ≤1/8 and Content Slop = 0/8
- Final response includes the deck path and the serve command

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
| `references/html-templates.md` | All slide-type HTML templates + `<index.html>` patterns + Motion patterns (`scripts/base.css` is the source of truth for CSS) | Phase 5 |
| `references/resources.md` | CDN libs with full URLs and usage examples | Phase 4 + 5 |
| `references/slide-rules.md` | **Master rule set**: content, visual, layout, narrative, UX, delivery, anti-patterns, named formulas | Phase 3 + 4 + 5 |

## Script files

The `scripts/` folder holds the **copy-verbatim** templates. Every generated deck copies them as-is:

| File | Destination | Purpose |
|------|-------------|---------|
| `scripts/base.html` | `index.html` | Navigation controller (multi-iframe stage, name hashes, overview, presenter, HUD) |
| `scripts/slide.html` | `slides/*.html` | Per-slide template (4-region skeleton + LLM placeholders) |
| `scripts/base.css` | `css/base.css` | Layout primitives, type scale, slide-type rules, components, animations, print |
| `scripts/navbridge.js` | `js/navbridge.js` | Forwards iframe key events to parent — required in every slide |
| `scripts/presenter.js` | `js/presenter.js` | `P`-key speaker-notes popup, loaded by `scripts/base.html` |

**Rule:** copy verbatim, never paraphrase. Theme overrides go in `css/theme.css`; one-off layout helpers live in the slide's local `<style>`. Never edit a copied script after the copy. Step-by-step in `references/05-implementation.md` Step 5.

Per-slide library inits (`hljs.highlightAll()`, `marked.parse()`, `mermaid.initialize()`, Motion calls) stay inline in the slides that need them — they're 1–3 lines each and not every slide uses them.
