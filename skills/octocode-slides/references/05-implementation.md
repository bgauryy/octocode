# Phase 5 — Implementation

**Role:** Implementation agent. Turn the outline, slide specs, and DESIGN.md into working HTML slides. Keep the build focused: finalize only the slide-ready content needed for each slide, then implement the HTML and review in small internal batches.

**Input:** `.content/brief.md` · `.content/outline.md` · `.content/slides/*.md` · `.content/DESIGN.md` · `css/base.css` + `css/theme.css`
**Output:** updated `.content/slides/NN-slug.md` specs · `slides/NN-slug.html` (implemented)

**Path contract — read before writing any file. All paths are inside `.octocode/slides/{{slideName}}/`:**
```
.octocode/slides/{{slideName}}/   ← deck root (serve from here)
├── index.html                    ← navigation controller
├── css/
│   ├── base.css
│   └── theme.css
├── js/
│   └── navbridge.js              ← keyboard bridge (must exist)
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

## Step 2 · Verify and finalize per-slide specs

Before writing any HTML, verify that every outline row has a matching `.content/slides/NN-slug.md` spec. The spec separates what the slide means from how it is implemented.

For each slide in `.content/outline.md`:

- Confirm the spec contains `Title`, `Description`, `Reasoning`, `Content`, `Data`, `Widgets`, `Graphs`, `Images`, and `UX / UI`
- Resolve `Status: needs source`, `needs asset`, or `revisit` before building, unless the slide intentionally uses an image placeholder
- Tighten body content to final slide text; move extra explanation into speaker notes
- Confirm the planned widget/graph/image can be implemented with the selected libraries and available assets

Track completion internally. Skip per-slide confirmations unless the user asked for progress at that granularity.

---

## Step 3 · Implement slides

For each slide, build directly from its `.content/slides/NN-slug.md` spec, starting from `scripts/slide.html`:

1. Copy `scripts/slide.html` as the starting point
2. Use `Title` for the on-slide heading and browser `<title>`
3. Use `Description` and `Reasoning` to preserve the slide's purpose and flow; do not let implementation drift from them
4. Use `Content`, `Data`, `Widgets`, `Graphs`, `Images`, and `UX / UI` to choose the exact markup and libraries
5. Replace all `<!-- LLM: ... -->` comments with actual content
6. Use the correct layout from `references/html-templates.md` for the slide type
7. Add CDN libraries if this slide needs them — check DESIGN.md libraries list AND the slide spec (`Graphs` → chart library, `Widgets` → Motion/Mermaid/code, Markdown body → marked.js)
8. Use Motion animation patterns from `references/html-templates.md` if appropriate
9. Write to `slides/NN-slug.html` (NOT `slides/slides/` — slides go directly in `slides/`)
10. Track completion internally

**Implementation rules:**
- CSS variables only (`var(--accent)`, `var(--t-title)`, etc.) — no hardcoded values
- Speaker notes go in `<aside class="speaker-notes">`
- Overflow → split into a new slide (update `.content/outline.md` and continue)
- For code slides: use highlight.js with the theme from DESIGN.md
- For markdown-content slides: use marked.js + `data-md` pattern
- For diagram / flow / architecture slides: use Mermaid.js
- For chart slides — pick the right library (one per slide; avoid loading two chart libs):
  - Bar / line / area / donut / scatter / radar → **Chart.js** (lightest, best default)
  - Heatmap / geo / treemap / candlestick → **ECharts**
  - Dense time-series (100+ data points) → **uPlot**
  - Polished look with minimal config → **ApexCharts**
  - Custom / bespoke / network layout → **D3.js**
  - Static comparison ≤6 bars → CSS-only `width: X%` bars, no library
- For KPI / counter / number countup: use **Motion** `animate(0, N, onUpdate)` — no chart library needed
- For progress bars: use **Motion** `animate(el, { width: ['0%', 'N%'] })` or CSS `@starting-style`
- `calc(-1 * clamp(...))` for any negated length instead of `-clamp(...)`
- Motion: load as `<script type="module">` at bottom of `<body>`
- **Treat the approved outline as the contract.** If implementation reveals a better title, split, or order, update `.content/outline.md` first and keep the Question-Answer chain intact.
- **Preserve the Question-Answer chain.** The `Flow logic` column in the outline is the contract. Each slide's heading should carry the meaning of that column — if the title drifts, the chain breaks.

**Image handling (check the slide spec's `Images` section first, then brief.md → Images inventory):**

All image files go in `assets/` at the deck root. Slides reference them as `../assets/filename.png` (one level up from `slides/`).

| Image status in brief | What to do in HTML |
|-----------------------|--------------------|
| `ready` — file path provided | `<img src="../assets/{{filename}}" alt="{{descriptive alt text}}">` |
| `placeholder` — user will provide later | Use `image-ph` (inline) or `image-ph-bleed` (full-bleed) from `references/html-templates.md` |
| Full-bleed `slide--image` with ready image | `<img src="../assets/{{filename}}">` + `<div class="image-overlay">` + optional `.image-caption` |
| Full-bleed `slide--image` with no image yet | `image-ph-bleed` div + `<div class="image-overlay">` + `.image-caption` |

Add a `data-expected` attribute with a plain-English description of the image when it helps review, especially for placeholders: `data-expected="{{what the image shows}}"`.

For full-bleed slides with images: the `.image-overlay` gradient div is **mandatory** — it ensures text in `.image-caption` remains legible regardless of the image content.

---

## Step 4 · Implementation loop

After every **5 slides**, run an internal mini-review. Pause for user feedback only when the user explicitly wants collaborative checkpoints, a missing asset blocks a slide, or a content decision cannot be inferred.

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

### 5a · Create js/navbridge.js

Write `js/navbridge.js` at the deck root (same level as `css/` and `slides/`). This script is included by every slide HTML and forwards keyboard navigation events from inside the iframe back to the parent `index.html` via `postMessage`, so arrow keys keep working after the user clicks anywhere inside a slide.

```javascript
/*
 * Slide → parent navigation bridge.
 * When a slide iframe has keyboard focus (after the user clicks anywhere
 * inside it), arrow keys fire on the iframe's document, not the parent
 * window. This script forwards those keys to the parent via postMessage
 * so the navigation controller in index.html can keep working.
 */
(function () {
  if (window.parent === window) return; // standalone, not embedded

  var NAV_KEYS = {
    ArrowLeft: 1, ArrowRight: 1, ArrowUp: 1, ArrowDown: 1,
    PageUp: 1, PageDown: 1, Home: 1, End: 1,
    ' ': 1, g: 1, G: 1, f: 1, F: 1
  };

  function isTypingTarget(el) {
    if (!el) return false;
    var tag = (el.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select') return true;
    return !!el.isContentEditable;
  }

  function hasTextSelection() {
    try {
      var sel = window.getSelection && window.getSelection();
      return !!(sel && String(sel).length > 0);
    } catch (_) { return false; }
  }

  function send(key) {
    try {
      window.parent.postMessage(
        { type: 'octocode-slides:nav', key: key },
        '*'
      );
    } catch (_) {}
  }

  document.addEventListener('keydown', function (e) {
    if (isTypingTarget(e.target)) return;
    if (!NAV_KEYS[e.key]) return;
    if (e.key === ' ' && hasTextSelection()) return;
    send(e.key);
    e.preventDefault();
  }, true);

  // Forward mouse activity so the parent HUD wakes up
  document.addEventListener('mousemove', function () {
    try { window.parent.postMessage({ type: 'octocode-slides:activity' }, '*'); }
    catch (_) {}
  }, { passive: true });
})();
```

### 5b · Build index.html

1. Start from `scripts/base.html`
2. Replace all `<!-- LLM: ... -->` comments with actual values
3. Fill `const slides = [...]` using the `{ path, hidden, name }` object format:
   - `path` — slide HTML file relative to `index.html` (e.g. `'slides/problem.html'`)
   - `name` — unique slug for URL hash (e.g. `'problem'` → `#problem`). **Do NOT use numbers** — playback order is controlled by the array, not filenames.
   - `hidden` — `true` to skip during playback and hide from overview grid
4. Keep entries in the order you want them shown — this array is the single source of truth for slide order.
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

- [ ] Every slide in the outline has a `.content/slides/NN-slug.md` file
- [ ] Every slide spec has `Title`, `Description`, `Reasoning`, `Content`, `Data`, `Widgets`, `Graphs`, `Images`, and `UX / UI`
- [ ] No slide spec remains `Status: needs source`, `needs asset`, or `revisit` unless the deck intentionally ships an image placeholder
- [ ] Every slide in the outline has a `slides/*.html` file (not `slides/slides/`)
- [ ] `js/navbridge.js` exists at the deck root (same level as `css/` and `slides/`)
- [ ] Every slide HTML file contains `<script src="../js/navbridge.js"></script>` immediately before `</body>`
- [ ] `const slides = [...]` in `index.html` uses `{ path, hidden, name }` objects — no plain strings, no numeric names
- [ ] All `path` values in the manifest start with `slides/` (e.g. `'slides/problem.html'`)
- [ ] All `name` values are unique slugs — not numbers, not filenames with extensions
- [ ] `index.html` is at the deck root (same level as `css/`, `js/`, and `slides/`)
- [ ] No slide HTML contains hardcoded colors, fonts, or pixel sizes
- [ ] Every CDN library listed in DESIGN.md is actually loaded in the slides that need it
- [ ] Each slide's `.slide` container uses flex layout (inherited from `base.css`); no slide content overflows at 1280×720

Pass to Phase 6 → read `references/06-review.md`. Start with Step 0 (Self-review).
