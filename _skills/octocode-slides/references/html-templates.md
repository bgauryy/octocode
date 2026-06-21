# HTML Templates Reference — Octocode Slides

Read during Phase 5 implementation, and consult during Phase 4 when a design choice depends on available layout patterns.

---

## Canonical slide skeleton

A slide can use up to **four regions** in this order. Only `.slide-content` is required — every other region is opt-in per slide. The skeleton is a contract for **where things go when present**, not a recipe forcing every slide into the same shape.

| Region | Class | Optional? | Purpose |
|---|---|---|---|
| Logo | `.slide-logo` | Optional | Brand mark, anchored top-right (absolute) |
| Header | `.slide-header` | Optional | `.title` + `.description` (muted subtitle) |
| Content | `.slide-content` | **Required** | Smart flex body — bullets, grid, chart, image, etc. |
| Footer | `.slide-footer` | Optional | Source line, page number, link, attribution |

**Use what serves the slide; omit the rest.** A title hero may only need `.slide-content`. A data slide may use all four. Variety across slides is good — what matters is that *when* a region appears, it sits in the same place and uses the same class.

```html
<div class="slide slide--{{TYPE}}">
  <!-- optional: brand mark, top-right -->
  <header class="slide-logo">
    <img src="../assets/logo.svg" alt="{{Brand}}">
  </header>

  <!-- optional: title + description -->
  <header class="slide-header">
    <h2 class="title">{{Slide title — claim sentence}}</h2>
    <p class="description">{{One-line description (optional)}}</p>
  </header>

  <!-- required: the body. Smart flex column by default. -->
  <main class="slide-content">
    {{body markup — see per-type templates below}}
  </main>

  <!-- optional: source / context / page / link -->
  <footer class="slide-footer">
    <span>{{Source or context}}</span>
    <span>{{Page or link}}</span>
  </footer>
</div>
```

`.slide-content` defaults to `display: flex; flex-direction: column; flex: 1; gap: var(--sp-3)`. Use these modifier classes when the default isn't right:

| Modifier | Effect | Use for |
|---|---|---|
| `slide-content--center` | Center along both axes + text-align center | Single number, single sentence, hero block |
| `slide-content--middle` | Center vertically only | Tall body that should sit in the middle |
| `slide-content--row` | Switch to flex-row, align stretched | Text + image side-by-side |
| `slide-content--grid-2` | Two equal columns | Comparisons, two-up cards |
| `slide-content--grid-3` | Three equal columns | KPI grid, three icons + labels |

Centered slide types (`title`, `section`, `quote`, `closing`) automatically center the entire stack — no modifier needed.

---

## Naming convention — classes and IDs

Every element targeted by animation, JS control, or external overrides must have a precise, unambiguous selector. Vague class names collide across slides when scripts run in `index.html` context or when a future deck reuses a component.

### Class rules for repetitive components

| Pattern | Use | Example |
|---------|-----|---------|
| `{component}-list` | Wrapping container for a group | `bullets`, `agenda-list`, `stat-grid`, `timeline` |
| `{component}-item` | Individual member of a list | `stat-item`, `tl-item`, `bar-row` |
| `{component}-{part}` | Named sub-part of an item | `stat-value`, `stat-label`, `tl-dot`, `tl-label`, `bar-label` |
| `{context}-col {context}-{role}` | Layout column scoped to a slide type | `cmp-col cmp-before` · `col two-col-left` · `col two-col-right` |
| `{slide-type}-{role}` | Unique element within one slide type | `closing-links`, `image-caption`, `chart-insight`, `code-caption` |

**Never use bare, context-free class names** — `col`, `item`, `panel`, `row` alone — as the sole identifier. Always pair them with a context qualifier (e.g. `col two-col-left`) so animation selectors stay unambiguous across a multi-slide deck.

### ID rules

Assign `id` only when:

- An animation counter needs a direct DOM target (`document.getElementById(...)` in `onUpdate`)
- A chart or diagram library initializes by element ID
- JS needs a reliable, unique anchor (focus management, presenter note jump target)

**Format: `{slide-slug}-{role}`** — e.g. `id="stats-kpi-1"`, `id="perf-chart"`, `id="timeline-root"`.

Never use bare numeric IDs (`id="1"`) or generic IDs (`id="chart"`, `id="kpi"`). They collide when a slide is scripted from the parent `index.html` or reused across decks.

---

## Slide file shell

Every `slides/slug.html` uses this shell. Slides fill the iframe 100%×100% — the stage in `index.html` handles all scaling. Add CDN `<link>` / `<script>` tags inside `<head>` only when the slide needs them.

**`js/navbridge.js` is required in every slide.** It propagates arrow-key events from the iframe back to the parent navigation controller via `postMessage`, so keyboard navigation keeps working after the user clicks inside a slide. The `<script>` tag is already included at the end of `scripts/slide.html` — do not remove it.

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{Slide Title}}</title>
  <link rel="stylesheet" href="../css/base.css">
  <link rel="stylesheet" href="../css/theme.css">
  <!-- Optional: add CDN libraries only when the slide needs them -->
</head>
<body>
<div class="slide slide--{{TYPE}}">
  <!-- Up to four regions: .slide-logo, .slide-header, .slide-content, .slide-footer -->
  <!-- Only .slide-content is required. -->
</div>
<aside class="speaker-notes">{{Speaker notes}}</aside>
<script src="../js/navbridge.js"></script>
</body>
</html>
```

---

## Slide type templates

All templates below use the **canonical 4-region skeleton**. Logo and footer regions are shown commented-out: include them when the deck calls for them and stay consistent about it. Description lines are shown as `{{Description (optional)}}` — fill them when the title needs reinforcement, leave the line out when it doesn't. Don't pad slides with descriptions just because the slot exists.

### title

Centered hero. The deck's first impression. `.slide-header` carries the display heading; `.slide-content` carries supporting copy.

```html
<div class="slide slide--title">
  <!-- <header class="slide-logo"><img src="../assets/logo.svg" alt="Brand"></header> -->
  <header class="slide-header">
    <p class="eyebrow fade-in">{{Event or tagline}}</p>
    <h1 class="display slide-up">{{Main Title}}</h1>
    <p class="description slide-up delay-1">{{Subtitle / description}}</p>
  </header>
  <main class="slide-content">
    <p class="meta fade-in delay-2">{{Author}} · {{Date}}</p>
  </main>
</div>
```

### agenda

```html
<div class="slide slide--agenda">
  <header class="slide-header">
    <h2 class="title fade-in">{{Agenda}}</h2>
    <!-- optional: <p class="description">{{One-line promise}}</p> -->
  </header>
  <main class="slide-content">
    <ol class="agenda-list">
      <li class="slide-up delay-1"><span class="num">01</span>{{Topic One}}</li>
      <li class="slide-up delay-2"><span class="num">02</span>{{Topic Two}}</li>
      <li class="slide-up delay-3"><span class="num">03</span>{{Topic Three}}</li>
    </ol>
  </main>
</div>
```

### section-header

```html
<div class="slide slide--section">
  <header class="slide-header">
    <p class="section-num fade-in">{{01}}</p>
    <h2 class="display slide-up">{{Section Title}}</h2>
    <p class="description slide-up delay-1">{{One supporting line}}</p>
  </header>
</div>
```

`.slide-header` is the only region used; the centered slide type vertically centers it.

### content

Header above, smart-flex body below.

```html
<div class="slide slide--content">
  <header class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
    <!-- optional: <p class="description">{{Why this matters}}</p> -->
  </header>
  <main class="slide-content">
    <ul class="bullets">
      <li class="slide-up delay-1">{{Point one — max 12 words}}</li>
      <li class="slide-up delay-2">{{Point two}}</li>
      <li class="slide-up delay-3">{{Point three}}</li>
      <!-- max 4 bullets -->
    </ul>
  </main>
</div>
```

### two-column

```html
<div class="slide slide--two-col">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content slide-content--grid-2">
    <!-- `col` keeps base.css styling; `two-col-left` gives animation/JS a precise target -->
    <div class="col two-col-left slide-up delay-1">
      <h3 class="col-heading two-col-heading">{{Left heading}}</h3>
      <ul class="two-col-bullets"><li>{{Point}}</li><li>{{Point}}</li></ul>
    </div>
    <div class="col two-col-right slide-up delay-2">
      <h3 class="col-heading two-col-heading">{{Right heading}}</h3>
      <ul class="two-col-bullets"><li>{{Point}}</li><li>{{Point}}</li></ul>
    </div>
  </main>
</div>
```

### quote

```html
<div class="slide slide--quote">
  <main class="slide-content">
    <p class="quote-mark fade-in" aria-hidden="true">"</p>
    <blockquote class="quote-text slide-up">{{Quote — max 30 words}}</blockquote>
    <cite class="quote-attr fade-in delay-2">— {{Name, Title}}</cite>
  </main>
</div>
```

### code (with highlight.js)

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{Slide Title}}</title>
  <link rel="stylesheet" href="../css/base.css">
  <link rel="stylesheet" href="../css/theme.css">
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11/build/styles/github-dark.min.css">
  <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11/build/highlight.min.js"></script>
</head>
<body>
<div class="slide slide--code">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content">
    <pre class="code-block slide-up delay-1"><code class="language-{{js|python|typescript|bash|go}}">{{
// code here — max 20 lines
}}</code></pre>
    <p class="code-caption fade-in delay-2">{{One-line insight}}</p>
  </main>
</div>
<aside class="speaker-notes">{{Speaker notes}}</aside>
<script>hljs.highlightAll();</script>
<script src="../js/navbridge.js"></script>
</body>
</html>
```

### chart (HTML+CSS bar chart — no JS library)

```html
<div class="slide slide--chart">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content">
    <div class="bar-chart slide-up delay-1" role="img" aria-label="{{description}}">
      <div class="bar-row">
        <span class="bar-label">{{Label A}}</span>
        <div class="bar" style="--pct:80%"><span>{{Value}}</span></div>
      </div>
      <div class="bar-row">
        <span class="bar-label">{{Label B}}</span>
        <div class="bar" style="--pct:55%"><span>{{Value}}</span></div>
      </div>
      <div class="bar-row">
        <span class="bar-label">{{Label C}}</span>
        <div class="bar" style="--pct:35%"><span>{{Value}}</span></div>
      </div>
    </div>
    <p class="chart-insight fade-in delay-3">{{Key takeaway}}</p>
  </main>
</div>
```

### timeline

```html
<div class="slide slide--timeline">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content">
    <ol class="timeline">
      <li class="tl-item slide-up delay-1">
        <span class="tl-dot"></span>
        <div><strong class="tl-label">{{2022 / Q1}}</strong> {{Description}}</div>
      </li>
      <li class="tl-item slide-up delay-2"><!-- repeat --></li>
    </ol>
  </main>
</div>
```

### comparison

```html
<div class="slide slide--comparison">
  <header class="slide-header">
    <h2 class="title fade-in">{{Before / After}}</h2>
  </header>
  <main class="slide-content">
    <div class="comparison">
      <div class="cmp-col cmp-before slide-up delay-1">
        <h3>{{Before}}</h3>
        <ul><li>{{Point}}</li><li>{{Point}}</li></ul>
      </div>
      <div class="cmp-divider fade-in delay-2"></div>
      <div class="cmp-col cmp-after slide-up delay-3">
        <h3>{{After}}</h3>
        <ul><li>{{Point}}</li><li>{{Point}}</li></ul>
      </div>
    </div>
  </main>
</div>
```

### closing

```html
<div class="slide slide--closing">
  <header class="slide-header">
    <h2 class="display slide-up">{{Thank You / Key Takeaway}}</h2>
    <p class="description fade-in delay-1">{{Call to action}}</p>
  </header>
  <main class="slide-content">
    <div class="closing-links fade-in delay-2">
      <a href="{{URL}}">{{Link text}}</a>
    </div>
  </main>
</div>
```

### stats (KPI / big numbers)

Use for 1–3 key metrics. Add a Motion counter animation when numbers should count up.

```html
<div class="slide slide--stats">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading — claim sentence}}</h2>
  </header>
  <main class="slide-content">
    <div class="stat-grid">
      <!-- id="{slide-slug}-kpi-{n}" — required for animation counter targeting -->
      <div class="stat-item pop-in delay-1">
        <span class="stat-value" id="{{slide-slug}}-kpi-1">{{Number or symbol}}</span>
        <span class="stat-label">{{Label}}</span>
      </div>
      <div class="stat-item pop-in delay-2">
        <span class="stat-value" id="{{slide-slug}}-kpi-2">{{Number or symbol}}</span>
        <span class="stat-label">{{Label}}</span>
      </div>
      <div class="stat-item pop-in delay-3">
        <span class="stat-value" id="{{slide-slug}}-kpi-3">{{Number or symbol}}</span>
        <span class="stat-label">{{Label}}</span>
      </div>
    </div>
    <p class="stat-caption fade-in delay-4">{{Source or context}}</p>
  </main>
</div>
```

### image (full-bleed background)

Full-bleed slides are special: they bypass the standard regions and use absolute layering.

```html
<div class="slide slide--image">
  <img src="{{path/to/image.jpg}}" alt="{{Descriptive alt text}}">
  <div class="image-overlay" aria-hidden="true"></div>
  <div class="image-caption slide-up delay-1">
    <p class="display">{{Short headline}}</p>
    <p class="description">{{Supporting line}}</p>
  </div>
</div>
```

### image-placeholder (inline — user will provide image later)

Use for `content`, `two-col`, or `chart` slides that need an inline image the user hasn't provided yet. Replace `.image-ph` with `<img>` once the file is available.

```html
<div class="slide slide--content">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content">
    <!-- IMAGE PLACEHOLDER — replace with <img src="PATH" alt="ALT"> when provided -->
    <div class="image-ph slide-up delay-1" data-expected="{{image description}}">
      <div class="image-ph-inner">
        <svg width="44" height="44" viewBox="0 0 44 44" fill="none" aria-hidden="true" style="opacity:0.5">
          <rect x="1" y="1" width="42" height="42" rx="5" stroke="currentColor" stroke-width="1.5" stroke-dasharray="5 3"/>
          <circle cx="16" cy="17" r="4" stroke="currentColor" stroke-width="1.5"/>
          <path d="M3 33l10-8 7 6 7-7 14 9" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>
        </svg>
        <p class="image-ph-label">PLACEHOLDER: {{image description}}</p>
      </div>
    </div>
  </main>
</div>
```

### image-placeholder (full-bleed — for slide--image type when image not yet provided)

```html
<div class="slide slide--image">
  <div class="image-ph-bleed" aria-label="Image placeholder: {{image description}}">
    [ PLACEHOLDER: {{image description}} ]
  </div>
  <div class="image-overlay" aria-hidden="true"></div>
  <div class="image-caption slide-up delay-1">
    <p class="display">{{Short headline}}</p>
    <p class="description">{{Supporting line}}</p>
  </div>
</div>
```

### markdown-content (renders .md source via marked.js)

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{Slide Title}}</title>
  <link rel="stylesheet" href="../css/base.css">
  <link rel="stylesheet" href="../css/theme.css">
  <script src="https://cdn.jsdelivr.net/npm/marked/lib/marked.umd.js"></script>
  <style>
    .md-body h1, .md-body h2 { font-family: var(--font-head); color: var(--accent); margin-bottom: var(--sp-3); }
    .md-body h1 { font-size: var(--t-title); }
    .md-body h2 { font-size: var(--t-sub); }
    .md-body p  { font-size: var(--t-body); color: var(--text); margin-bottom: var(--sp-2); }
    .md-body ul, .md-body ol { padding-left: var(--sp-4); font-size: var(--t-body); }
    .md-body li { margin-bottom: var(--sp-1); }
    .md-body code { font-family: var(--font-mono); font-size: var(--t-code); background: var(--code-bg); padding: 0.1em 0.4em; border-radius: var(--r-sm); }
    .md-body blockquote { border-left: 3px solid var(--accent); padding-left: var(--sp-3); color: var(--muted); font-style: italic; }
    .md-body strong { color: var(--accent); font-weight: 700; }
  </style>
</head>
<body>
<div class="slide slide--content">
  <header class="slide-header">
    <h2 class="title fade-in">{{Heading}}</h2>
  </header>
  <main class="slide-content">
    <div class="md-body slide-up delay-1" data-md>
{{Paste raw Markdown content here — agent fills this from source files}}
    </div>
  </main>
</div>
<aside class="speaker-notes">{{Speaker notes}}</aside>
<script>
  marked.setOptions({ breaks: true, gfm: true });
  document.querySelectorAll('[data-md]').forEach(el => {
    el.innerHTML = marked.parse(el.textContent.trim());
  });
</script>
<script src="../js/navbridge.js"></script>
</body>
</html>
```

---

## Region usage — guidelines, not rules

- **Logo / footer rhythm is usually deck-wide, but exceptions are fine.** Pick a posture in Phase 4 (always-on, always-off, or "everywhere except hero/section breaks") and stick to it. Don't sprinkle them onto random slides — that creates noise rather than rhythm.
- **Header is per-slide.** Use `.display` for hero/section/closing slides, `.title` for content/code/chart/etc. If the slide's *whole point* is the body — a single quote, one giant number, a full-bleed image — omit the header.
- **Description is optional.** Reach for it when the title needs a "why this matters" line. Skip it when the title already lands.
- **Footer carries facts about the slide, not the message of the slide.** Sources, page numbers, links, attribution — yes. Extra bullets or claims — no.
- **The four classes are how you stay consistent. The *content* of each region is up to the slide.** Two slides using `.slide-content--grid-2` can look completely different — different visuals, different rhythm, different focus. Same skeleton, different shape.

---

## Motion animation patterns

Use these `<script type="module">` blocks at the bottom of a slide's `<body>`. They replace or augment the CSS animation classes from `base.css`.

### Staggered entrance (lists, bullets, cards)
```html
<script type="module">
  import { animate, stagger } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  animate(
    '.bullets li, .agenda-list li, .stat-item, .tl-item',
    { opacity: [0, 1], y: [16, 0] },
    { delay: stagger(0.12), duration: 0.45, easing: [0.22, 1, 0.36, 1] }
  );
</script>
```

### Title + description sequence
```html
<script type="module">
  import { timeline } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  timeline([
    ['.display',     { opacity: [0, 1], y: [-14, 0] }, { duration: 0.5 }],
    ['.description', { opacity: [0, 1] },              { duration: 0.35, at: '+0.1' }],
    ['.meta',        { opacity: [0, 1] },              { duration: 0.3,  at: '+0.08' }],
  ]);
</script>
```

### Animated counter (stat / KPI slides)
```html
<script type="module">
  import { animate } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  // Use the slide-slug-scoped ID from the stats template (e.g. 'results-kpi-1')
  animate(0, {{TARGET_NUMBER}}, {
    duration: 1.4,
    easing: [0.22, 1, 0.36, 1],
    onUpdate(v) {
      document.getElementById('{{slide-slug}}-kpi-1').textContent =
        Math.round(v).toLocaleString();
    }
  });
</script>
```

### Bar chart fill animation (augments CSS .bar::after)
```html
<script type="module">
  import { animate, stagger } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  document.querySelectorAll('.bar').forEach((bar, i) => {
    const target = bar.style.getPropertyValue('--pct');
    bar.style.setProperty('--pct', '0%');
    setTimeout(() => {
      animate(0, parseFloat(target), {
        duration: 0.8,
        delay: i * 0.1,
        easing: [0.22, 1, 0.36, 1],
        onUpdate(v) { bar.style.setProperty('--pct', v + '%'); }
      });
    }, 200);
  });
</script>
```

### In-view reveal (for timelines / diagrams that still fit on one slide)
```html
<script type="module">
  import { animate, inView } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  inView('.tl-item', ({ target }) => {
    animate(target, { opacity: [0, 1], x: [-20, 0] }, { duration: 0.4 });
  }, { margin: '-10% 0px' });
</script>
```

**Rules:**
- Motion loads from ESM CDN — always `type="module"`.
- Prefer `timeline()` for multi-step sequences; `animate()` + `stagger()` for lists.
- Don't combine Motion entrance animations with CSS `.fade-in`/`.slide-up` on the same element — pick one.
- Respect `prefers-reduced-motion` — wrap Motion calls in `if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches)`.

---


## `css/base.css` source

**Copy `scripts/base.css` verbatim to `css/base.css` at the deck root.** Do not paraphrase, do not regenerate from memory — `scripts/base.css` is the canonical source of truth for layout primitives, type scale, slide-type rules, component classes, animations, and the print/PDF media query.

Override the override surface in `css/theme.css` only — colors, fonts, and the type-scale `clamp()` ranges if the design calls for it. Never edit `css/base.css` after the copy.

**The override surface (defined in `:root` in `scripts/base.css`):**

```css
/* css/theme.css overrides any of these; do not redefine layout rules. */
:root {
  /* Colors */
  --bg --surface --border --accent --text --muted --code-bg --code-text
  /* Fonts */
  --font-head --font-body --font-mono
  /* Type scale (clamp ranges) */
  --t-display --t-title --t-sub --t-body --t-small --t-code
  /* Spacing (8px grid) */
  --sp-1 --sp-2 --sp-3 --sp-4 --sp-6 --sp-8 --pad
  /* Radius */
  --r-sm --r-md --r-lg
  /* Motion */
  --fast --base --slow --ease
}
```

**What `scripts/base.css` provides (do not duplicate elsewhere):**
- Reset + slide canvas (1280×720, `overflow: hidden`)
- Canonical 4-region skeleton (`.slide-logo`, `.slide-header`, `.slide-content`, `.slide-footer`)
- Smart-flex modifiers (`--center`, `--middle`, `--row`, `--grid-2`, `--grid-3`)
- Typography classes (`.display`, `.title`, `.description`, `.subtitle`, `.eyebrow`, `.meta`)
- Slide-type rules (`.slide--title`, `.slide--section`, `.slide--quote`, `.slide--closing`, `.slide--stats`, `.slide--agenda`, `.slide--image`)
- Component classes (`.bullets`, `.agenda-list`, `.code-block`, `.bar-chart`, `.timeline`, `.comparison`, `.stat-grid`, `.image-ph`, `.image-ph-bleed`, `.image-overlay`, `.image-caption`)
- Animations (`.fade-in`, `.slide-up`, `.pop-in`, `.delay-1` … `.delay-4`) + `prefers-reduced-motion`
- `@media print` for 1280×720 PDF export

If a class is missing for a one-off layout, add it to the slide's local `<style>` — do not edit `scripts/base.css` or `css/base.css` after the copy.

---

## `index.html` — navigation controller

**Always copy from `scripts/base.html`** — it is the canonical source of truth. The condensed reference below shows the key patterns; use the full template for actual implementation.

### Slide manifest format

```javascript
const slides = [
  // Each entry: { path, hidden, name }
  //   path   – slide HTML file relative to index.html
  //   name   – unique slug for URL hash (e.g. 'problem' → #problem)
  //            Do NOT use numbers — playback order is controlled by this array.
  //   hidden – true = skip during playback AND hide from overview grid
  { path: 'slides/title.html',    hidden: false, name: 'title' },
  { path: 'slides/problem.html',  hidden: false, name: 'problem' },
  { path: 'slides/solution.html', hidden: false, name: 'solution' },
  { path: 'slides/closing.html',  hidden: false, name: 'closing' },
];
```

Key rules for the manifest:
- **Filename numbers do NOT control order** — the array position does. Files can be named `slides/title.html` without a numeric prefix.
- **`name` must be unique** across all visible (non-hidden) slides.
- **`hidden: true`** skips the slide during playback and hides it from the overview grid, but keeps the file in the deck (useful for draft slides or extended-edition content).
- **Name-based hash navigation**: `#problem` jumps to the slide where `name === 'problem'`. Legacy numeric hashes (`#5`) still work for backwards compatibility.

### Navbridge integration

The parent `index.html` uses a **single `handleKey()` function** as the sole navigation handler:

- When the **parent window** has focus → `document.addEventListener('keydown', handleKey, true)` fires directly. Step-aware nav keys (`→`, `↓`, Space, `←`, `↑`) are first posted into the active iframe as `{ type: 'octocode-slides:key', key }`, so `animation.js` can reveal or hide a step before slide navigation happens.
- When the **iframe** has focus (user clicked inside a slide) → `js/navbridge.js` inside the slide posts `{ type: 'octocode-slides:nav', key }` only after the slide has no step left to consume in that direction.
- The parent's message listener calls `handleKey({ key, passthrough: true })`; `passthrough:true` means the key already passed through the active iframe and should now advance or retreat the deck.

Do NOT attach a second `keydown` listener to the iframe — that would double-fire and advance two slides per key press.

```javascript
window.addEventListener('message', function (event) {
  var data = event.data;
  if (!data || typeof data !== 'object') return;
  if (data.type === 'octocode-slides:nav' && data.key) {
    handleKey({ key: data.key, passthrough: true, preventDefault: function () {} });
  } else if (data.type === 'octocode-slides:activity') {
    showHud();
  } else if (data.type === 'octocode-slides:presenter-goto') {
    go(clampIndex(data.index));
  }
});
```

### HUD (keyboard hint pill)

The HUD fades in on mouse move / key press, fades out after 1.6 s:

```javascript
let hudTimer;
function showHud() {
  hud.classList.add('show');
  clearTimeout(hudTimer);
  hudTimer = setTimeout(() => hud.classList.remove('show'), 1600);
}
window.addEventListener('mousemove', showHud);
```

**After generating all slides:** fill `const slides = [...]` with every `{ path, hidden, name }` entry in presentation order.
