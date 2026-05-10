# Phase 5 — Implementation

**Role:** Implementation agent. Turn the outline, inline slide notes, and DESIGN.md into working HTML slides. Keep the build focused: finalize only the slide-ready content needed for each slide, then implement the HTML and review in small internal batches.

**Input:** `.content/request.md` · `.content/outline.md` · `.content/DESIGN.md` · `css/base.css` + `css/theme.css`
**Output:** `slides/slug.html` (one per outline row)

**Path contract:** see `SKILL.md → Output structure`. All paths are inside `.octocode/slides/{{slideName}}/`. Slides reference `../css/*`, `../js/*`, `../assets/*`. `index.html` references `slides/*.html`. No `slides/slides/` nesting.

---

## Step 1 · Read references

Read these files now (in parallel):
- `references/html-templates.md` — slide-type HTML templates + Motion patterns + `index.html` controller (the canonical CSS lives in `scripts/base.css`)
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

### 5a · Copy verbatim scripts

Run these copies; never paraphrase from memory:

```bash
cp scripts/navbridge.js js/navbridge.js
cp scripts/presenter.js js/presenter.js
# css/base.css was already copied by Phase 4 Step 7; verify it exists
```

If `css/base.css` is missing for any reason, copy it now: `cp scripts/base.css css/base.css`. Theme overrides remain in `css/theme.css`.

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

### 5c · Wire pointer chrome on `index.html` (default: on)

If `DESIGN.md → Pointer & click feedback` is present, copy the wiring snippet from `references/resources.md → Pointer & Click Feedback → Wiring on index.html` into `index.html` (just before `</body>`). The snippet is the canonical implementation — do not paraphrase.

**Mandatory rules** (verified in Phase 6):
- Loaded on `index.html` only — **never** inside a slide HTML (each slide is a separate iframe document).
- `<click-spark>` wrapper: `pointer-events: none; z-index: 5` so HUD (z-index 50) and progress (z-index 10) stay clickable.
- Keep the `pointer: coarse` and `prefers-reduced-motion` short-circuits in the snippet.

Skip this step when `DESIGN.md → Libraries` says `Pointer chrome: off`.

### 5d · Write README.md

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

## Step 6 · Hand-off to Phase 6

Quick sanity gate — only the items unique to Phase 5 (the structural deck shape). The full technical, design, and content review lives in `references/06-review.md`.

- [ ] Every outline row has a matching `slides/slug.html` (no `slides/slides/`)
- [ ] `js/navbridge.js`, `js/presenter.js`, `css/base.css`, `css/theme.css` all exist at the deck root
- [ ] No `[NEEDS SOURCE]` / `needs asset` / `revisit` rows remain unless they ship as labeled placeholders
- [ ] No slide HTML contains hardcoded colors, fonts, or pixel sizes (CSS variables only)

Pass to Phase 6 → read `references/06-review.md`. Start with Step 0 (Self-review).
