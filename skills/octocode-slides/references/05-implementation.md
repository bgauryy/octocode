# Phase 5 — Implementation

**Role:** Implementation agent. Turn the outline, inline slide notes, and DESIGN.md into working HTML slides. Keep the build focused: finalize only the slide-ready content needed for each slide, then implement the HTML and review in small internal batches.

**Input:** `.content/request.md` · `.content/outline.md` · `.content/DESIGN.md` · `css/base.css` + `css/theme.css`
**Output:** `slides/slug.html` (one per outline row)

**Path contract — read before writing any file. All paths are inside `.octocode/slides/{{slideName}}/`:**
```
.octocode/slides/{{slideName}}/   ← deck root (serve from here)
├── index.html                    ← navigation controller
├── css/
│   ├── base.css
│   └── theme.css
├── js/
│   ├── navbridge.js              ← keyboard bridge (required)
│   └── presenter.js              ← presenter popup (optional)
├── assets/                       ← images and media referenced by slides
│   └── (images go here)
└── slides/
    └── *.html                    ← one HTML file per slide
```
Each slide uses paths one level up: `../css/base.css`, `../js/navbridge.js`, `../assets/image.png`. `index.html` uses `slides/slug.html` (one level down). Keep slides out of `slides/slides/` — there is no double-nesting in this structure.

---

## Step 1 · Read references

Read these files now (in parallel):
- `references/html-templates.md` — all slide type HTML templates + base.css boilerplate + Motion patterns
- `references/resources.md` — CDN URLs for any library listed in DESIGN.md
- `references/slide-rules.md` §§1, 5 — Content rules and Logical Flow rules (required by Global Rule 9)

---

## Step 2 · Verify the outline is implementation-ready

Before writing any HTML, read `.content/outline.md` fully. For each row in the outline table:

- Confirm the title is a claim sentence (not a topic label) — if not, fix it now
- Check the `Source` column: any `[NEEDS SOURCE]` must be resolved or the slide flagged with a visible placeholder in HTML
- Check `Slide notes` for any widget, chart data, image path, or layout instruction specific to that slide
- If a chart slide has no data in the outline, ask the user for the values before building that slide
- If an image slide has no path and no placeholder instruction, use the `image-ph` pattern from `references/html-templates.md`

Track completion internally. Do not ask the user for per-slide confirmation unless a specific data gap blocks progress.

---

## Step 3 · Implement slides

For each row in `.content/outline.md`, build directly from the row's data and any matching `Slide notes` entry:

1. Copy `scripts/slide.html` as the starting point — **always**
2. Use the row's title as the on-slide heading and browser `<title>`
3. Use the row's type, key content, source, and flow logic as the implementation contract
4. Check `Slide notes` for that slug — use any widget/chart/image/layout instructions found there
5. Replace all `<!-- LLM: ... -->` comments with actual content from `request.md`
6. Use the correct layout from `references/html-templates.md` for the slide type
7. Add CDN libraries only if this slide needs them — check `DESIGN.md → Libraries` then the slide's `Slide notes`
8. Use Motion animation patterns from `references/html-templates.md` where the slide type calls for it
9. Write to `slides/slug.html` — slug must match the `Slug` column in the outline table (NOT `slides/slides/`)
10. Track completion internally

**Implementation rules:**
- CSS variables only (`var(--accent)`, `var(--t-title)`, etc.) — no hardcoded values
- Speaker notes go in `<aside class="speaker-notes">`
- Overflow → split into a new slide (update `.content/outline.md` and continue)
- For code slides: use highlight.js with the theme from DESIGN.md
- For markdown-content slides: use marked.js + `data-md` pattern
- For diagram / flow / architecture slides: use Mermaid.js
- For chart / KPI / progress widgets — the outline already names the library (Phase 3 Step 4). If it doesn't, decide from `references/resources.md → Data Visualization — Library Decision` and update `outline.md` before implementing. One chart lib per slide; never two.
- `calc(-1 * clamp(...))` for any negated length instead of `-clamp(...)`
- Motion: load as `<script type="module">` at bottom of `<body>`
- **The outline is the contract.** If implementation reveals a better title, split, or order — update `.content/outline.md` first, then build to the updated version.
- **Preserve the Question-Answer chain.** The `Flow logic` column in the outline is the contract. Each slide's heading should carry the meaning of that column — if the title drifts, the chain breaks.

**Image handling (check the slide's `Slide notes` in `outline.md`, then `request.md → Images`):**

All image files go in `assets/` at the deck root. Slides reference them as `../assets/filename.png` (one level up from `slides/`).

| Image status in brief | What to do in HTML |
|-----------------------|--------------------|
| `ready` — file path provided | `<img src="../assets/{{filename}}" alt="{{descriptive alt text}}">` |
| `placeholder` / `[IMAGE PLACEHOLDER]` — user will provide later | Use the `PLACEHOLDER` component: `image-ph` (inline) or `image-ph-bleed` (full-bleed) from `references/html-templates.md` |
| Full-bleed `slide--image` with ready image | `<img src="../assets/{{filename}}">` + `<div class="image-overlay">` + optional `.image-caption` |
| Full-bleed `slide--image` with no image yet | `image-ph-bleed` div + `<div class="image-overlay">` + `.image-caption` |

For any missing image, do not search, download, generate, or silently substitute an image. Render the `PLACEHOLDER` component and add a `data-expected` attribute with a plain-English description of the image: `data-expected="{{what the image shows}}"`. The user can replace it later with a real file.

For full-bleed slides with images: the `.image-overlay` gradient div is **mandatory** — it ensures text in `.image-caption` remains legible regardless of the image content.

---

## Step 3b · Template alignment check (run after every slide, not at the end)

Every slide must be structurally identical in its scaffolding. Check each slide before moving to the next:

| Check | Pass condition | Fix |
|-------|---------------|-----|
| Started from `scripts/slide.html` | `<link rel="stylesheet" href="../css/base.css">` exists | Re-copy template, do not patch inline |
| Theme loaded | `<link rel="stylesheet" href="../css/theme.css">` exists | Add the link |
| Navbridge loaded | `<script src="../js/navbridge.js"></script>` immediately before `</body>` | Add in correct position |
| Local CSS is justified | Only slide-specific layout helpers live in `<style>`; colors/fonts/sizes still use design tokens | Move reusable styles to `base.css` or `theme.css` |
| CSS variables only | No `color: #hex` or `font-family: "..."` inline on any element | Replace all hardcoded values with `var(--token)` |
| Slide class set | `<div class="slide slide--{{type}}">` matches the slide type in the outline row | Correct the class |
| No inline `style` width/height for layout | Dimensions use CSS classes or `var()` | Extract to class |
| No scroll at 1280×720 | Content fits without `overflow-y: auto` being needed | Split slide or reduce content |

**If any slide fails a check:** fix it immediately before writing the next slide. Do not accumulate debt.

---

## Step 4 · Implementation loop

Run an internal mini-review at natural break points — section boundaries, after every 5–8 slides, or whenever density / type pattern changes. The point is to catch drift early, not to hit a fixed cadence.

Pause for user feedback only when the user explicitly wants collaborative checkpoints, a missing asset blocks a slide, or a content decision can't be inferred from `outline.md` + `request.md`.

When pausing for user feedback, use:

```
Slides {{N–M}} implemented ({{current}}/{{total}} total).

Self-check before showing you:
- Titles: all claim sentences (not topic labels)?
- Flow: each slide answers the previous question, raises the next?
- Overflow: all content fits 1280×720 without scrolling?
- Variables: no hardcoded colors/fonts?

Reply "continue" to build the next batch, or give feedback.
```

In fast/delegated mode, continue after the internal mini-review without waiting. If the user gives feedback, fix the flagged slides before continuing.
If the user says "continue", run the self-check against the next batch as you build it.

This loop runs until all slides in the outline are implemented.

---

## Step 5 · Build index.html and js/navbridge.js

Once all slides are implemented:

### 5a · Create js/ scripts

**Required — `js/navbridge.js`:** Copy `scripts/navbridge.js` to `js/navbridge.js` at the deck root. Forwards keyboard events from the focused iframe to the parent navigation controller. Do not rewrite from memory.

**Required — `js/presenter.js`:** Copy `scripts/presenter.js` to `js/presenter.js` at the deck root. The default `scripts/base.html` controller loads it and wires `P` to presenter notes. Do not hand-wire a second presenter implementation in generated decks.

### 5b · Build index.html

1. Start from `scripts/base.html`
2. Replace all `<!-- LLM: ... -->` comments with actual values
3. Fill `const slides = [...]` using the `{ path, hidden, name }` object format:
   - `path` — slide HTML file relative to `index.html` (e.g. `'slides/problem.html'`)
   - `name` — unique slug for URL hash (e.g. `'problem'` → `#problem`). **Do NOT use numbers** — playback order is controlled by the array, not filenames.
   - `hidden` — `true` to skip during playback and hide from overview grid
4. Keep entries in the order you want them shown — this array is the single source of truth for slide order.

Do not replace `scripts/base.html` with a single-iframe controller. The current controller preloads slide iframes for grid thumbnails, uses name-based hashes, forwards iframe keyboard events through navbridge, and wires `P` to presenter notes.
5. Write to `index.html` (at deck root — same level as `css/`, `js/`, and `slides/`)

```javascript
// Example manifest — replace with actual slides:
const slides = [
  { path: 'slides/title.html',    hidden: false, name: 'title' },
  { path: 'slides/problem.html',  hidden: false, name: 'problem' },
  { path: 'slides/solution.html', hidden: false, name: 'solution' },
  { path: 'slides/closing.html',  hidden: false, name: 'closing' },
];
```

### 5b² · Wire pointer chrome on `index.html` (default: on)

If `DESIGN.md → Pointer & click feedback` is present (default for live presentations — see `references/04-design.md` Step 5b), wire the two libraries on the parent **only**. Skip this step when `DESIGN.md → Libraries` says `Pointer chrome: off`.

Insert just before `</body>` in `index.html`:

```html
<!-- ── Pointer chrome (parent only — never per slide) ─── -->
<click-spark style="--click-spark-color: var(--accent); position: fixed; inset: 0; pointer-events: none; z-index: 5;"></click-spark>
<script type="module">
  import { followingDotCursor } from "https://unpkg.com/cursor-effects@latest/dist/esm.js";
  import "https://cdn.jsdelivr.net/gh/hexagoncircle/click-spark/click-spark.js";
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
  if (!matchMedia('(pointer: coarse)').matches && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
    new followingDotCursor({ color: accent });
  }
</script>
```

**Rules:**
- Loaded on `index.html` **only** — never inside a slide HTML. Slides are separate iframe documents; loading there would spawn one cursor per slide and break continuity across transitions.
- `z-index` must sit **above** the slide stage but **below** the HUD / progress bar / counter (which use `z-index: 10`+ in `scripts/base.html`). The example uses `z-index: 5`.
- The HUD must stay clickable, so keep `pointer-events: none` on the `<click-spark>` wrapper — clicks pass through to the underlying element while still emitting a spark at the click point.
- `pointer: coarse` short-circuit disables the custom cursor on touch devices.
- `prefers-reduced-motion` short-circuit is defensive — the library already honours it, but the deck's policy is to respect it at every layer.
- For offline-friendly delivery, vendor both files into `js/vendor/cursor-effects.esm.js` and `js/vendor/click-spark.js` and switch the imports to relative paths.

Verification: open `index.html`, see the cursor follow with a small lag, click anywhere on the chrome — a 6-spoke spark should fire in `--accent`. Press `G` to enter overview; the custom cursor should disappear (or you can wrap the `new followingDotCursor(...)` in a `body.overview` guard if it interferes with thumbnail clicking).

### 5c · Write README.md

Write `README.md` (at deck root):

```markdown
# {{Deck Title}}

Serve: `npx serve .octocode/slides/{{slideName}}`
Then open: http://localhost:3000

Keys: `→` next · `←` prev · `Space` next · `G` overview grid · `F` fullscreen

Edit a slide: `slides/*.html`
Change theme: `css/theme.css` — all slides update automatically
Reorder slides: edit the `slides` array in `index.html`
```

---

## Step 6 · Final implementation check

Before handing off to Phase 6:

- [ ] Every slide in `outline.md` has been implemented as `slides/slug.html`
- [ ] No outline row remains `[NEEDS SOURCE]`, `needs asset`, or `revisit` unless the deck intentionally ships a labeled placeholder
- [ ] Every slide in the outline has a `slides/*.html` file (not `slides/slides/`)
- [ ] `js/navbridge.js` exists at the deck root (same level as `css/` and `slides/`)
- [ ] `js/presenter.js` exists at the deck root (same level as `css/` and `slides/`)
- [ ] Every slide HTML file contains `<script src="../js/navbridge.js"></script>` immediately before `</body>`
- [ ] `const slides = [...]` in `index.html` uses `{ path, hidden, name }` objects — no plain strings, no numeric names
- [ ] All `path` values in the manifest start with `slides/` (e.g. `'slides/problem.html'`)
- [ ] All `name` values are unique slugs — not numbers, not filenames with extensions
- [ ] `index.html` is at the deck root (same level as `css/`, `js/`, and `slides/`)
- [ ] No slide HTML contains hardcoded colors, fonts, or pixel sizes
- [ ] Every CDN library listed in DESIGN.md is actually loaded in the slides that need it
- [ ] Each slide's `.slide` container uses flex layout (inherited from `base.css`); no slide content overflows at 1280×720

Pass to Phase 6 → read `references/06-review.md`. Start with Step 0 (Self-review).
