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
    <div class="col slide-up delay-1">
      <h3 class="col-heading">{{Left}}</h3>
      <ul><li>{{Point}}</li><li>{{Point}}</li></ul>
    </div>
    <div class="col slide-up delay-2">
      <h3 class="col-heading">{{Right}}</h3>
      <ul><li>{{Point}}</li><li>{{Point}}</li></ul>
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
      <div class="stat-item pop-in delay-1">
        <span class="stat-value" id="kpi1">{{Number or symbol}}</span>
        <span class="stat-label">{{Label}}</span>
      </div>
      <div class="stat-item pop-in delay-2">
        <span class="stat-value" id="kpi2">{{Number or symbol}}</span>
        <span class="stat-label">{{Label}}</span>
      </div>
      <div class="stat-item pop-in delay-3">
        <span class="stat-value" id="kpi3">{{Number or symbol}}</span>
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
  animate(0, {{TARGET_NUMBER}}, {
    duration: 1.4,
    easing: [0.22, 1, 0.36, 1],
    onUpdate(v) {
      document.getElementById('kpi').textContent =
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

## `css/base.css` boilerplate

Copy verbatim. Variables are overridden by `theme.css`.

```css
/* base.css — layout, variables, all slide rules */

:root {
  /* --- Colors (defaults; theme.css overrides) --- */
  --bg:        #0D1117;
  --surface:   #161B22;
  --border:    #30363D;
  --accent:    #39D353;
  --text:      #E6EDF3;
  --muted:     #7D8590;
  --code-bg:   #0D1117;
  --code-text: #79C0FF;

  /* --- Fonts (theme.css sets these) --- */
  --font-head: system-ui, sans-serif;
  --font-body: system-ui, sans-serif;
  --font-mono: 'Courier New', monospace;

  /* --- Type scale (responsive, no px overrides) --- */
  --t-display:  clamp(2.75rem, 5.5vw, 4.25rem);  /* 44–68px — title/section */
  --t-title:    clamp(1.6rem,  3.5vw, 2.5rem);   /* 26–40px — slide headings */
  --t-sub:      clamp(1.15rem, 2.5vw, 1.65rem);  /* 18–26px — descriptions, sub-headings */
  --t-body:     clamp(1rem,    1.8vw, 1.2rem);   /* 16–19px — bullets, paragraphs */
  --t-small:    clamp(0.75rem, 1.2vw, 0.9rem);   /* 12–14px — captions, footers */
  --t-code:     clamp(0.82rem, 1.4vw, 1rem);     /* 13–16px — code blocks */

  /* --- Spacing (8px grid) --- */
  --sp-1: 0.5rem;   --sp-2: 1rem;     --sp-3: 1.5rem;
  --sp-4: 2rem;     --sp-6: 3rem;     --sp-8: 4rem;

  /* --- Slide canvas --- */
  --pad: var(--sp-8);

  /* --- Radius --- */
  --r-sm: 4px;  --r-md: 8px;  --r-lg: 16px;

  /* --- Motion --- */
  --fast: 150ms;  --base: 300ms;  --slow: 600ms;
  --ease: cubic-bezier(0.22, 1, 0.36, 1);
}

/* Reset */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* Slide fills its iframe 100%×100% — scaling is on the stage wrapper */
html { width: 100%; height: 100%; }
body {
  width: 100%; height: 100%;
  font-family: var(--font-body);
  color: var(--text);
  background: var(--bg);
  line-height: 1.6;
  overflow: hidden; /* slides should never scroll — split content if it overflows */
}

/* ============================================================ */
/* CANONICAL SLIDE SKELETON                                     */
/* Every slide uses up to four regions, in this order:          */
/*   .slide-logo     optional · brand mark, top-right (absolute) */
/*   .slide-header   optional · .title + .description           */
/*   .slide-content  REQUIRED · smart flex body                 */
/*   .slide-footer   optional · source / page / link            */
/* ============================================================ */

.slide {
  width: 100%; height: 100%;
  padding: var(--pad);
  display: flex;
  flex-direction: column;
  background: var(--bg);
  position: relative;
  overflow: hidden;
}

/* Region: logo (absolute top-right) */
.slide-logo {
  position: absolute;
  top: var(--sp-3);
  right: var(--sp-4);
  height: clamp(20px, 2.2vw, 30px);
  display: flex;
  align-items: center;
  z-index: 2;
  opacity: 0.85;
}
.slide-logo img,
.slide-logo svg { height: 100%; width: auto; display: block; }

/* Region: header (title + description, stacked) */
.slide-header {
  flex-shrink: 0;
  margin-bottom: var(--sp-4);
  display: flex;
  flex-direction: column;
  gap: var(--sp-2);
}

/* Region: content (smart flex body, fills remaining height) */
.slide-content {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--sp-3);
}

/* Region: footer (source, page number, link, attribution) */
.slide-footer {
  flex-shrink: 0;
  margin-top: var(--sp-4);
  padding-top: var(--sp-3);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-3);
  font-size: var(--t-small);
  color: var(--muted);
}
.slide-footer a { color: var(--accent); text-decoration: none; }
.slide-footer a:hover { text-decoration: underline; }

/* Smart flex modifiers for .slide-content (opt-in when default isn't right) */
.slide-content--center  { justify-content: center; align-items: center; text-align: center; }
.slide-content--middle  { justify-content: center; }
.slide-content--row     { flex-direction: row; align-items: stretch; gap: var(--sp-6); }
.slide-content--grid-2  { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-6); }
.slide-content--grid-3  { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--sp-4); }

/* ---------- Typography classes ---------- */
.display {
  font-family: var(--font-head);
  font-size: var(--t-display);
  font-weight: 700;
  line-height: 1.12;
  letter-spacing: -0.02em;
  color: var(--accent);
}
.title {
  font-family: var(--font-head);
  font-size: var(--t-title);
  font-weight: 700;
  line-height: 1.2;
  color: var(--text);
}
.description {
  font-family: var(--font-body);
  font-size: var(--t-sub);
  font-weight: 400;
  line-height: 1.4;
  color: var(--muted);
}
.subtitle { font-size: var(--t-sub); color: var(--muted); }
.eyebrow {
  font-size: var(--t-small);
  color: var(--accent);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  margin-bottom: var(--sp-2);
}
.meta { font-size: var(--t-small); color: var(--muted); margin-top: var(--sp-4); }

/* ---------- Slide type rules ---------- */

/* Centered types: vertically center the entire region stack.
   .slide-content stops being greedy so the stack sizes to its content. */
.slide--title,
.slide--section,
.slide--quote,
.slide--closing { justify-content: center; }

.slide--title    .slide-content,
.slide--section  .slide-content,
.slide--quote    .slide-content,
.slide--closing  .slide-content { flex: 0 0 auto; }

/* Title and closing also center horizontally */
.slide--title,
.slide--closing { text-align: center; align-items: center; }
.slide--title   .slide-header,
.slide--title   .slide-content,
.slide--closing .slide-header,
.slide--closing .slide-content { align-items: center; }
.slide--title   .meta { display: flex; justify-content: center; gap: var(--sp-2); }

/* Stats and agenda: header sits above; content vertically centered in body */
.slide--stats   .slide-content,
.slide--agenda  .slide-content { justify-content: center; }

/* Quote: extra horizontal padding for breathing room */
.slide--quote { padding: var(--sp-8) calc(var(--sp-8) * 1.5); text-align: center; }
.slide--quote .slide-content { align-items: center; }

/* Section: vertically centered, left-aligned (default text-align) */
.slide--section .slide-header { gap: var(--sp-3); }
.section-num { font-size: var(--t-small); color: var(--accent); font-weight: 700; letter-spacing: 0.1em; }

/* ---------- Component classes (used inside .slide-content) ---------- */

/* agenda */
.agenda-list { list-style: none; display: flex; flex-direction: column; gap: var(--sp-3); }
.agenda-list li { display: flex; align-items: center; gap: var(--sp-3); font-size: var(--t-body); }
.num { font-size: var(--t-small); color: var(--accent); font-weight: 700; min-width: 2.5rem; }

/* bullets (content slide) */
.bullets { list-style: none; display: flex; flex-direction: column; gap: var(--sp-3); }
.bullets li {
  font-size: var(--t-body);
  padding-left: var(--sp-4);
  position: relative;
}
.bullets li::before { content: '→'; position: absolute; left: 0; color: var(--accent); }

/* two-column */
.col-heading { font-size: var(--t-sub); font-weight: 700; color: var(--accent); margin-bottom: var(--sp-3); }
.col ul { list-style: none; display: flex; flex-direction: column; gap: var(--sp-2); font-size: var(--t-body); }

/* quote */
.quote-mark { font-size: 6rem; line-height: 0.8; color: var(--accent); opacity: 0.25; font-family: Georgia, serif; }
.quote-text { font-size: var(--t-sub); font-style: italic; line-height: 1.5; margin: var(--sp-4) 0; }
.quote-attr { font-size: var(--t-small); color: var(--muted); }

/* code */
.code-block {
  background: var(--code-bg);
  border: 1px solid var(--border);
  border-radius: var(--r-md);
  padding: var(--sp-4);
  font-family: var(--font-mono);
  font-size: var(--t-code);
  color: var(--code-text);
  line-height: 1.6;
  overflow: hidden;
  flex: 1;
}
.code-caption { font-size: var(--t-small); color: var(--muted); margin-top: var(--sp-2); }

/* chart */
.bar-chart { display: flex; flex-direction: column; gap: var(--sp-3); flex: 1; }
.bar-row { display: flex; align-items: center; gap: var(--sp-3); }
.bar-label { font-size: var(--t-small); min-width: 8rem; color: var(--muted); }
.bar { flex: 1; height: 2.25rem; background: var(--surface); border-radius: var(--r-sm); position: relative; overflow: hidden; }
.bar::after {
  content: '';
  position: absolute; left: 0; top: 0;
  width: var(--pct, 0%); height: 100%;
  background: var(--accent);
  border-radius: var(--r-sm);
  transition: width var(--slow) var(--ease);
}
.bar span { position: absolute; right: var(--sp-2); top: 50%; transform: translateY(-50%); font-size: var(--t-small); font-weight: 700; z-index: 1; }
.chart-insight { font-size: var(--t-body); color: var(--muted); font-style: italic; margin-top: var(--sp-3); }

/* timeline */
.timeline { list-style: none; display: flex; flex-direction: column; gap: 0; position: relative; }
.timeline::before { content: ''; position: absolute; left: 0.55rem; top: 0; bottom: 0; width: 2px; background: var(--border); }
.tl-item { display: flex; gap: var(--sp-3); padding-bottom: var(--sp-4); position: relative; }
.tl-dot { width: 1.1rem; height: 1.1rem; min-width: 1.1rem; border-radius: 50%; background: var(--accent); margin-top: 0.25rem; z-index: 1; }
.tl-label { font-size: var(--t-small); color: var(--accent); font-weight: 700; display: block; }

/* comparison */
.comparison { display: grid; grid-template-columns: 1fr auto 1fr; gap: var(--sp-4); flex: 1; }
.cmp-divider { width: 1px; background: var(--border); }
.cmp-col h3 { font-size: var(--t-sub); font-weight: 700; margin-bottom: var(--sp-3); }
.cmp-before h3 { color: var(--muted); }
.cmp-after  h3 { color: var(--accent); }
.cmp-col ul { list-style: none; display: flex; flex-direction: column; gap: var(--sp-2); font-size: var(--t-body); }

/* closing */
.closing-links { display: flex; gap: var(--sp-4); justify-content: center; margin-top: var(--sp-4); }
.closing-links a { color: var(--accent); font-size: var(--t-body); text-decoration: none; border-bottom: 1px solid currentColor; padding-bottom: 2px; }

/* stats */
.stat-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--sp-6); padding: var(--sp-4) 0; flex: 1; align-items: center; }
.stat-item { display: flex; flex-direction: column; align-items: center; gap: var(--sp-2); }
.stat-value { font-family: var(--font-head); font-size: var(--t-display); font-weight: 700; color: var(--accent); line-height: 1; letter-spacing: -0.02em; }
.stat-label { font-size: var(--t-body); color: var(--muted); text-align: center; }
.stat-caption { font-size: var(--t-small); color: var(--muted); font-style: italic; margin-top: var(--sp-4); text-align: center; }

/* image (full-bleed) */
.slide--image { padding: 0; overflow: hidden; }
.slide--image img,
.slide--image video { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
.image-overlay { position: absolute; inset: 0; background: linear-gradient(to bottom, transparent 40%, rgba(0,0,0,0.72) 100%); }
.image-caption { position: absolute; bottom: var(--sp-8); left: var(--sp-8); right: var(--sp-8); }
.slide--image .image-caption .display { color: #fff; text-shadow: 0 2px 12px rgba(0,0,0,0.5); }
.slide--image .image-caption .description { color: rgba(255,255,255,0.78); margin-top: var(--sp-2); }

/* image placeholder — used when user has not yet provided an image file */
.image-ph {
  flex: 1;
  border: 1.5px dashed var(--border);
  border-radius: var(--r-md);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface);
  min-height: 180px;
  color: var(--muted);
}
.image-ph-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--sp-2);
  text-align: center;
  padding: var(--sp-4);
}
.image-ph-label { font-size: var(--t-small); max-width: 28ch; line-height: 1.4; }

/* full-bleed placeholder — for slide--image type when image not yet provided */
.image-ph-bleed {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface);
  color: var(--muted);
  font-size: var(--t-small);
  font-family: var(--font-mono);
  letter-spacing: 0.04em;
  border: 2px dashed var(--border);
}

/* Speaker notes — hidden in presentation */
.speaker-notes { display: none; }

/* ---------- Animations ---------- */
@keyframes fadeIn  { from { opacity: 0 } to { opacity: 1 } }
@keyframes slideUp { from { opacity: 0; transform: translateY(20px) } to { opacity: 1; transform: none } }
@keyframes popIn   { from { opacity: 0; transform: scale(0.94) } to { opacity: 1; transform: none } }

.fade-in  { animation: fadeIn  var(--base) var(--ease) both; }
.slide-up { animation: slideUp var(--base) var(--ease) both; }
.pop-in   { animation: popIn   var(--fast) var(--ease) both; }

.delay-1  { animation-delay: 100ms; }
.delay-2  { animation-delay: 220ms; }
.delay-3  { animation-delay: 340ms; }
.delay-4  { animation-delay: 460ms; }

@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: 0.01ms !important; transition-duration: 0.01ms !important; }
}

/* Print / PDF export (headless Chrome: 1280×720 pages) */
@media print {
  html, body { width: 1280px; height: 720px; overflow: hidden; }
  .slide { width: 1280px; height: 720px; page-break-after: always; }
  @page { size: 1280px 720px; margin: 0; }
}
```

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

- When the **parent window** has focus → `document.addEventListener('keydown', handleKey, true)` fires directly.
- When the **iframe** has focus (user clicked inside a slide) → `js/navbridge.js` inside the slide posts `{ type: 'octocode-slides:nav', key }` and the parent's `window.addEventListener('message', ...)` calls `handleKey()`.

Do NOT attach a second `keydown` listener to the iframe — that would double-fire and advance two slides per key press.

```javascript
window.addEventListener('message', function (event) {
  var data = event.data;
  if (!data || typeof data !== 'object') return;
  if (data.type === 'octocode-slides:nav' && data.key) {
    handleKey({ key: data.key, preventDefault: function () {} });
  } else if (data.type === 'octocode-slides:activity') {
    showHud();
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
