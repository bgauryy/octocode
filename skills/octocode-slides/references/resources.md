# Slide Libraries & Design Resources

CDN libraries and design references available to any slide. The agent chooses what fits the deck — nothing here is mandatory unless the slide type needs it.

---

## Markdown Rendering

### marked.js — Markdown → HTML, zero dependencies
**Rating:** ★★★★☆ — solid workhorse; only needed when slide content is Markdown-sourced
**CDN:** `https://cdn.jsdelivr.net/npm/marked/lib/marked.umd.js`
**Repo:** [markedjs/marked](https://github.com/markedjs/marked) | 35k+ stars
**Use when:** A content slide sources text from a `.md` file, or the user provides long-form content as Markdown.

```html
<script src="https://cdn.jsdelivr.net/npm/marked/lib/marked.umd.js"></script>
<div class="slide-body" data-md>
# Heading
- point one
- point two
</div>
<script>
  document.querySelectorAll('[data-md]').forEach(el => {
    el.innerHTML = marked.parse(el.textContent.trim());
  });
</script>
```

**Configuration for presentations:**
```js
marked.setOptions({
  breaks: true,      // newline → <br>
  gfm: true,         // GitHub Flavored Markdown
});
```

---

## Animation

### CSS-native stagger — zero dependencies (Chrome + Safari; Firefox in progress)
**Rating:** ★★★★★ — best default for bullet/card stagger; no import, no bundle, adapts to DOM changes automatically
**Spec:** [CSS Wrapped 2025](https://chrome.dev/css-wrapped-2025/) · [Smashing Mag deep-dive](https://www.smashingmagazine.com/2025/12/state-logic-native-power-css-wrapped-2025/)
**Use when:** Bullets, cards, or tiles need a staggered entrance. Replaces Motion.dev stagger for most slide use cases — no import, adapts dynamically to DOM changes.

```css
/* Add to slide <style>. Works with any list of siblings. */
.bullet {
  transition: opacity 0.25s ease, translate 0.25s ease;
  transition-delay: calc(0.1s * (sibling-index() - 1)); /* 1-based, subtract 1 so first = 0s */
  @starting-style {
    opacity: 0;
    translate: 0 0.75em;
  }
}
```

**Gate with `@supports` for older browsers:**
```css
@supports (transition-delay: calc(sibling-index() * 0s)) {
  .bullet { /* stagger rules */ }
}
```

---

### Motion — Production-grade JS animations
**Rating:** ★★★★★ — essential for counters, timelines, and spring physics; nothing else matches its API quality
**CDN (ESM):** `https://cdn.jsdelivr.net/npm/motion@latest/+esm`
**Docs:** [motion.dev/docs/quick-start](https://motion.dev/docs/quick-start)
**Repo:** [motiondotdev/motion](https://github.com/motiondotdev/motion) | 26k+ stars
**Use when:** Timelines, number counters, spring physics, or slide-local transitions that CSS can't express.

**Counter / number animation:**
```html
<script type="module">
  import { animate } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  animate(0, 2_400_000, {
    duration: 1.6,
    easing: [0.22, 1, 0.36, 1],
    onUpdate(v) {
      document.getElementById('counter').textContent = Math.round(v).toLocaleString();
    }
  });
</script>
```

**Timeline sequence (title → subtitle → body):**
```html
<script type="module">
  import { timeline } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  timeline([
    ['.slide-title',    { opacity: [0, 1], y: [-12, 0] }, { duration: 0.4 }],
    ['.slide-subtitle', { opacity: [0, 1] },               { duration: 0.3, at: '+0.1' }],
    ['.slide-body',     { opacity: [0, 1], y: [8, 0] },   { duration: 0.4, at: '+0.1' }],
  ]);
</script>
```

**Stagger (use CSS-native first; fall back to this when `sibling-index()` unsupported):**
```html
<script type="module">
  import { animate, stagger } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";
  animate('.bullet', { opacity: [0, 1], y: [16, 0] }, { delay: stagger(0.12), duration: 0.45, easing: [0.22, 1, 0.36, 1] });
</script>
```

---

### GSAP — Complex timelines and SVG choreography
**Rating:** ★★★☆☆ — overkill for most slides; reach for it only when Motion.dev and CSS both fall short
**CDN:** `https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.5/gsap.min.js`
**Docs:** [gsap.com/docs](https://gsap.com/docs/)
**Use when:** SVG path animations or complex choreography where Motion.dev is insufficient. Avoid scroll-driven patterns inside slides because slide content should remain fixed within the frame.

```html
<script src="https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.5/gsap.min.js"></script>
<script>
  gsap.from('.slide-body > *', { opacity: 0, y: 20, stagger: 0.12, duration: 0.5, ease: 'power2.out' });
</script>
```

---

**Animation decision table:**

| Situation | Use |
|-----------|-----|
| Simple fade / slide-up on load | CSS `.fade-in`, `.slide-up` from base.css |
| Staggered bullets/cards (modern browsers) | CSS `sibling-index()` + `@starting-style` |
| Staggered bullets/cards (broad browser support) | Motion `stagger()` |
| Number counters, progress fills | Motion `animate(0, N, onUpdate)` |
| Sequenced multi-element choreography | Motion `timeline()` |
| SVG path animation or complex choreography | GSAP |
| Spring / physics-based motion | Motion spring easing |

---

## Slide Transitions (View Transitions API)

**Rating:** ★★★★☆ — powerful and zero-dependency; iframe caveat limits the simplest path, but SPA pattern works cleanly
**No library needed — browser-native.**
**MDN:** [Using the View Transition API](https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API/Using)
**Recipes:** [Piccalilli — practical VT examples](https://piccalil.li/blog/some-practical-examples-of-view-transitions-to-elevate-your-ui/)
**Toolkit:** [vtbag.dev](https://vtbag.dev/) — "Bag of Tricks" for view transitions (named patterns, debugging, demos)

**SPA path — wrap iframe swap in `index.html`:**

```js
// In index.html navigation controller — wrap slide change in a transition
function goToSlide(n) {
  if (!document.startViewTransition) { updateSlide(n); return; }
  document.startViewTransition(() => updateSlide(n));
}
```

**Directional transitions — modern approach (`:active-view-transition-type`):**
```js
// Pass a direction type so CSS can target forward vs backward separately
function goToSlide(n, direction = 'forward') {
  if (!document.startViewTransition) { updateSlide(n); return; }
  document.startViewTransition({
    update: () => updateSlide(n),
    types: [direction]          // 'forward' or 'backward'
  });
}
```

```css
/* In index.html <style> — direction-aware keyframes */
@keyframes slide-out-left  { to   { transform: translateX(-100%); opacity: 0; } }
@keyframes slide-in-right  { from { transform: translateX( 100%); opacity: 0; } }
@keyframes slide-out-right { to   { transform: translateX( 100%); opacity: 0; } }
@keyframes slide-in-left   { from { transform: translateX(-100%); opacity: 0; } }

:active-view-transition-type(forward) {
  &::view-transition-old(root) { animation: slide-out-left  0.3s ease; }
  &::view-transition-new(root) { animation: slide-in-right  0.3s ease; }
}
:active-view-transition-type(backward) {
  &::view-transition-old(root) { animation: slide-out-right 0.3s ease; }
  &::view-transition-new(root) { animation: slide-in-left  0.3s ease; }
}

/* Gate reduced-motion */
@media (prefers-reduced-motion: reduce) {
  ::view-transition-old(root), ::view-transition-new(root) { animation: none; }
}
```

> **What VT animates in this architecture:** `document.startViewTransition` in `index.html` snapshots the **parent `#stage` wrapper** (the iframe container). The slide content *inside* the iframe is captured as a pixel snapshot. This is correct and visually convincing — the animation happens at the parent level, not inside the iframe.

**MPA / cross-document path (only for non-iframe decks):**
```css
/* Only use when each slide is a top-level page (no iframe wrapper). */
@view-transition { navigation: auto; }

@media (prefers-reduced-motion: reduce) {
  @view-transition { navigation: none; }
}
```

**Named shared elements (logo/title that morphs between slides):**
```css
/* Apply to the *same* element in old and new slide — must be unique per transition */
.slide-logo { view-transition-name: deck-logo; contain: layout; }
```

> **iframe caveat:** cross-document VT and `view-transition-name` on elements *inside* iframes do not propagate to the parent. Use the SPA path (`document.startViewTransition` in `index.html`) and name elements on the **parent** (e.g. a HUD logo or progress bar) for morphing effects.

---

## Code Syntax Highlighting

### highlight.js — Syntax highlighting, 200+ languages
**Rating:** ★★★★★ — drop-in, theme-matched, no config required; mandatory for any code slide
**CDN:**
```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11/build/styles/github-dark.min.css">
<script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11/build/highlight.min.js"></script>
```
**Repo:** [highlightjs/highlight.js](https://github.com/highlightjs/highlight.js) | 24k+ stars
**Use when:** A code slide needs real syntax coloring (not just a monospace block). Better than a CSS `.language-*` class alone.

```html
<pre><code class="language-typescript">async function fetchUser(id: string): Promise<User> {
  const res = await fetch(`/api/users/${id}`);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}</code></pre>
<script>
  document.querySelectorAll('pre code').forEach(el => hljs.highlightElement(el));
</script>
```

**Suggested themes by background energy — pick what fits your palette:**

| Background | Recommended | Alt |
|------------|-------------|-----|
| Dark cool / neutral | `github-dark` | `monokai-sublime` |
| Dark warm / very dark | `tokyo-night-dark` | `nord` |
| Light | `github` | `xcode` |
| Minimal / low-contrast | `ascetic` | `base16/one-light` |

---

## Data Visualization — Library Decision

Pick the lightest library that delivers the chart type needed. Avoid loading multiple chart libraries in one slide.

| Chart need | Library | Why |
|------------|---------|-----|
| Bar, line, area, donut, scatter, radar | **Chart.js** | Lightest CDN, built-in animation, official CDN support |
| Heatmap, geo/map, treemap, candlestick, graph | **ECharts** | Broadest type catalog; handles complex multi-panel layouts |
| Dense time-series, 10k+ points, perf-critical | **uPlot** | Smallest footprint, vanilla IIFE, zero dependencies |
| Polished multi-type with strong defaults | **ApexCharts** | SVG-based, real-time hooks, wide type catalog |
| Custom SVG/bespoke layouts, networks, projections | **D3.js** | Full control, no defaults; high authoring cost |
| CSS bar comparison (≤6 bars, static) | **CSS only** | No library needed; use `width: X%` + `--accent` bars |

**KPI / counter / progress widgets (no chart library needed):**

| Widget | Tool |
|--------|------|
| Number count-up | Motion `animate(0, N, onUpdate)` (see Animation section) |
| Progress bar | Motion `animate(el, { width: ['0%', 'N%'] })` or CSS `@starting-style` |
| Sparkline (mini trend line) | Chart.js mini canvas — see pattern below |
| Static bar comparison ≤6 bars | CSS only — see pattern below |

---

### CSS-only bar chart — no library needed

Use when the comparison is static, ≤6 bars, and no interactivity is required. Avoids a Chart.js import entirely.

```html
<div class="bar-chart">
  <div class="bar-row">
    <span class="bar-label">Q1</span>
    <div class="bar-track">
      <div class="bar-fill" style="--pct: 45%"><span>45%</span></div>
    </div>
  </div>
  <div class="bar-row">
    <span class="bar-label">Q2</span>
    <div class="bar-track">
      <div class="bar-fill" style="--pct: 72%"><span>72%</span></div>
    </div>
  </div>
  <div class="bar-row">
    <span class="bar-label">Q3</span>
    <div class="bar-track">
      <div class="bar-fill" style="--pct: 58%"><span>58%</span></div>
    </div>
  </div>
</div>

<style>
.bar-chart { display: grid; gap: var(--sp-3); }
.bar-row   { display: grid; grid-template-columns: 5rem 1fr; align-items: center; gap: var(--sp-3); }
.bar-label { font-size: var(--t-small); color: var(--muted); text-align: right; }
.bar-track { background: var(--surface); border-radius: var(--r-sm); overflow: hidden; height: 2.2rem; }
.bar-fill  {
  width: var(--pct);
  height: 100%;
  background: var(--accent);
  border-radius: var(--r-sm);
  display: flex; align-items: center; padding-left: var(--sp-2);
  font-size: var(--t-small); font-weight: 600; color: var(--bg);
  /* entrance animation — no library */
  animation: bar-grow 0.6s cubic-bezier(0.22, 1, 0.36, 1) both;
}
@keyframes bar-grow { from { width: 0; opacity: 0; } to { width: var(--pct); opacity: 1; } }
</style>
```

---

### Sparkline — mini trend line beside a KPI number

Use alongside a `stats`-type slide to show directional trend without a full chart. Requires Chart.js.

```html
<div class="kpi-block">
  <span class="kpi-number" id="kpi">0</span>
  <canvas id="spark" width="160" height="48"></canvas>
</div>

<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
<script type="module">
  import { animate } from "https://cdn.jsdelivr.net/npm/motion@latest/+esm";

  /* count-up */
  animate(0, 2_847_000, {
    duration: 1.4, easing: [0.22, 1, 0.36, 1],
    onUpdate(v) { document.getElementById('kpi').textContent = Math.round(v).toLocaleString(); }
  });

  /* sparkline */
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
  new Chart(document.getElementById('spark'), {
    type: 'line',
    data: {
      labels: Array(8).fill(''),
      datasets: [{
        data: [38, 45, 41, 60, 54, 71, 68, 82],
        borderColor: accent,
        borderWidth: 2,
        tension: 0.4,
        fill: false,
        pointRadius: 0,
      }]
    },
    options: {
      responsive: false,
      animation: false,
      plugins: { legend: { display: false }, tooltip: { enabled: false } },
      scales: { x: { display: false }, y: { display: false } },
    }
  });
</script>

<style>
.kpi-block  { display: flex; align-items: center; gap: var(--sp-4); }
.kpi-number { font-size: var(--t-display); font-weight: 700; color: var(--accent); }
</style>
```

---

### Chart.js — Bar, line, area, donut, scatter, radar
**Rating:** ★★★★☆ — best default for standard chart types; easy theming via CSS variables; 11M weekly downloads
**CDN:** `https://cdn.jsdelivr.net/npm/chart.js`
**Repo:** [chartjs/Chart.js](https://github.com/chartjs/Chart.js) | 65k+ stars
**Use when:** Standard bar, line, area, donut, scatter, or radar chart. The CSS bar template is not enough. Real data with labels and animation.

```html
<canvas id="chart" width="800" height="380"></canvas>
<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
<script>
  new Chart(document.getElementById('chart'), {
    type: 'bar',
    data: {
      labels: ['Q1', 'Q2', 'Q3', 'Q4'],
      datasets: [{
        label: 'Revenue ($M)',
        data: [12, 19, 8, 24],
        backgroundColor: getComputedStyle(document.documentElement)
          .getPropertyValue('--accent').trim(),
      }]
    },
    options: {
      responsive: false,
      animation: { duration: 800, easing: 'easeOutQuart' },
      plugins: { legend: { display: false } },
      scales: { y: { grid: { color: getComputedStyle(document.documentElement).getPropertyValue('--border').trim() } } }
    }
  });
</script>
```

---

### ECharts — Heatmaps, geo, treemaps, complex dashboards
**Rating:** ★★★★☆ — broadest chart type catalog; right choice when Chart.js cannot produce the chart type; 2.7M weekly downloads
**CDN:** `https://cdn.jsdelivr.net/npm/echarts/dist/echarts.min.js`
**Repo:** [apache/echarts](https://github.com/apache/echarts) | 61k+ stars
**Use when:** Heatmap, geo/map, treemap, candlestick, graph/network, calendar chart, or any multi-series complex layout that Chart.js cannot render.

```html
<div id="chart" class="chart-surface"></div>
<script src="https://cdn.jsdelivr.net/npm/echarts/dist/echarts.min.js"></script>
<script>
  const chartEl = document.getElementById('chart');
  const styles = getComputedStyle(document.documentElement);
  const chart = echarts.init(chartEl, null, { renderer: 'canvas' });
  chart.setOption({
    backgroundColor: 'transparent',
    tooltip: { trigger: 'item' },
    series: [{
      type: 'heatmap',
      data: [/* [col, row, value] */],
      label: { show: true },
      emphasis: { itemStyle: { shadowBlur: 10 } }
    }],
    visualMap: {
      min: 0, max: 100,
      calculable: true,
      inRange: {
        color: [
          styles.getPropertyValue('--surface').trim(),
          styles.getPropertyValue('--accent').trim()
        ]
      }
    }
  });
</script>
<style>
  .chart-surface { width: 100%; height: min(58vh, 26rem); }
</style>
```

---

### uPlot — Dense time-series and performance-critical charts
**Rating:** ★★★★☆ — smallest footprint for time-series; vanilla IIFE, no dependencies; 694K weekly downloads
**CDN (JS):** `https://cdn.jsdelivr.net/npm/uplot/dist/uPlot.iife.min.js`
**CDN (CSS):** `https://cdn.jsdelivr.net/npm/uplot/dist/uPlot.min.css`
**Repo:** [leeoniya/uPlot](https://github.com/leeoniya/uPlot) | 9k+ stars
**Use when:** A time-series slide has many data points (100+) and Chart.js feels sluggish, or when minimal bundle footprint is critical.

```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uplot/dist/uPlot.min.css">
<div id="chart" class="chart-surface"></div>
<script src="https://cdn.jsdelivr.net/npm/uplot/dist/uPlot.iife.min.js"></script>
<script>
  const chartEl = document.getElementById('chart');
  const styles = getComputedStyle(document.documentElement);
  const border = styles.getPropertyValue('--border').trim();
  const accent = styles.getPropertyValue('--accent').trim();
  const data = [
    [1700000000, 1700003600, 1700007200], // timestamps (x)
    [42, 58, 37],                          // series 1 (y)
  ];
  new uPlot({
    width: chartEl.clientWidth,
    height: chartEl.clientHeight,
    series: [
      {},
      { label: 'Requests/s', stroke: accent, width: 2 }
    ],
    axes: [
      { stroke: border, grid: { stroke: border } },
      { stroke: border, grid: { stroke: border } },
    ],
  }, data, chartEl);
</script>
<style>
  .chart-surface { width: 100%; height: min(54vh, 24rem); }
</style>
```

---

### ApexCharts — Polished multi-type charts with strong defaults
**Rating:** ★★★★☆ — SVG-based, clean defaults, real-time update API; good alternative to Chart.js when visual polish matters more than bundle size
**CDN:** `https://cdn.jsdelivr.net/npm/apexcharts`
**Repo:** [apexcharts/apexcharts.js](https://github.com/apexcharts/apexcharts.js) | 14k+ stars
**Use when:** You want professional-looking charts without custom styling effort — polished donut, area, radial bar, or mixed chart types.

```html
<div id="chart" class="chart-surface"></div>
<script src="https://cdn.jsdelivr.net/npm/apexcharts"></script>
<script>
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
  new ApexCharts(document.getElementById('chart'), {
    chart: { type: 'area', height: '100%', background: 'transparent', animations: { speed: 800 } },
    theme: { mode: 'dark' },
    colors: [accent],
    series: [{ name: 'Users', data: [31, 40, 28, 51, 42, 109, 100] }],
    xaxis: { categories: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] },
    dataLabels: { enabled: false },
    stroke: { curve: 'smooth', width: 2 },
  }).render();
</script>
<style>
  .chart-surface { width: 100%; height: min(54vh, 24rem); }
</style>
```

---

### D3.js — Custom SVG, networks, projections, bespoke charts
**Rating:** ★★★☆☆ — maximum control, highest authoring cost; use only when chart libraries cannot express the visual honestly
**CDN (ESM):** `https://cdn.jsdelivr.net/npm/d3@7/+esm`
**Repo:** [d3/d3](https://github.com/d3/d3) | 109k+ stars
**Use when:** Custom SVG layouts, force networks, geographic projections, unusual annotations, or bespoke chart grammar.

```html
<svg id="chart" class="chart-surface" role="img" aria-label="{{chart description}}"></svg>
<script type="module">
  import * as d3 from "https://cdn.jsdelivr.net/npm/d3@7/+esm";

  const svg = d3.select('#chart');
  const styles = getComputedStyle(document.documentElement);
  const accent = styles.getPropertyValue('--accent').trim();
  const muted = styles.getPropertyValue('--muted').trim();
  const width = svg.node().clientWidth;
  const height = svg.node().clientHeight;

  svg.attr('viewBox', `0 0 ${width} ${height}`);
  svg.append('circle')
    .attr('cx', width / 2)
    .attr('cy', height / 2)
    .attr('r', Math.min(width, height) / 4)
    .attr('fill', accent);
  svg.append('text')
    .attr('x', width / 2)
    .attr('y', height / 2)
    .attr('text-anchor', 'middle')
    .attr('fill', muted)
    .text('{{label}}');
</script>
<style>
  .chart-surface { width: 100%; height: min(54vh, 24rem); }
</style>
```

---

## Diagrams

### Mermaid.js — Diagrams from text
**Rating:** ★★★★★ — best-in-class for architecture/flow diagrams; text-based so content stays editable
**CDN:** `https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js`
**Repo:** [mermaid-js/mermaid](https://github.com/mermaid-js/mermaid) | 75k+ stars
**Use when:** A slide needs a flowchart, sequence diagram, Gantt, or architecture diagram.

```html
<div class="mermaid">
sequenceDiagram
  Client->>+API Gateway: POST /checkout
  API Gateway->>+Order Service: createOrder(cart)
  Order Service-->>-API Gateway: orderId: 4821
  API Gateway->>+Payment Service: charge(orderId)
  Payment Service-->>-API Gateway: status: ok
  API Gateway-->>-Client: 200 { orderId, status }
</div>
<script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
<script>
  // theme: 'dark' | 'default' | 'neutral' | 'forest' | 'base'
  // Pick based on --bg: dark themes → 'dark'; light themes → 'neutral' or 'default'
  mermaid.initialize({
    theme: 'dark',
    startOnLoad: true,
    flowchart: { curve: 'basis' }
  });
</script>
```

---

## PDF Export

### Decktape — universal HTML slide → PDF exporter
**Rating:** ★★★★☆ — works with any HTML slide framework; no dependency on the slide code itself
**Repo:** [astefanutti/decktape](https://github.com/astefanutti/decktape) | 2.4k+ stars
**Use when:** User asks to export the deck to PDF for sharing, printing, or archiving.

```bash
# Serve the deck first (required — Decktape fetches from a live URL)
npx serve .octocode/slides/{{slideName}}

# Export all slides to PDF
npx decktape http://localhost:3000 deck.pdf

# Export with explicit size (matches 1280×720 stage)
npx decktape --size 1280x720 http://localhost:3000 deck.pdf

# Export specific slides (1-indexed)
npx decktape --slides 1-10 http://localhost:3000 deck.pdf
```

**How it works:** Decktape drives a headless Chromium via Puppeteer, advances each slide via keyboard, and prints each to a PDF page. It auto-detects Reveal.js, Impress, and generic HTML decks.

**Alternative — browser `@media print`:**
`base.css` already defines `@media print { .slide { width: 1280px; height: 720px; page-break-after: always; } }`. Open `index.html` in Chrome → File → Print → Save as PDF. Lower fidelity than Decktape (no JS, no charts) but requires no installation.

---

## Pointer & Click Feedback

A two-piece pointer-chrome layer that makes a live deck feel like a debugger console: a themed cursor that follows the speaker, plus a short spark on every click. Both libs are tiny, vanilla, MIT, and themable through the deck's existing CSS variables. **Default: ON for live presentations; remove only when the brief is print/PDF-first, async, or the user opts out** (see `references/04-design.md` Step 5b).

> **Where they load:** parent `index.html` only — never per slide. Slides are iframes with separate documents, so loading these inside slides would create one cursor per slide and break the spark on slide chrome.

### tholman/cursor-effects — themable custom cursor (vanilla JS)
**Rating:** ★★★★☆ — battle-tested, ships ESM + UMD, already respects `prefers-reduced-motion` (matches the deck's animation policy)
**CDN (ESM):** `https://unpkg.com/cursor-effects@latest/dist/esm.js`
**Repo:** [tholman/cursor-effects](https://github.com/tholman/cursor-effects) | MIT
**Use when:** A live presentation or dark/tech deck wants a themed pointer (`followingDotCursor` / `trailingCursor` / `rainbowCursor`). Skip on print-first or async decks.

```html
<!-- in index.html, just before </body> -->
<script type="module">
  import { followingDotCursor } from "https://unpkg.com/cursor-effects@latest/dist/esm.js";
  const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim();
  new followingDotCursor({ color: accent });
</script>
```

**Available effects:** `followingDotCursor`, `trailingCursor`, `rainbowCursor`, `bubbleCursor`, `fairyDustCursor`, `ghostCursor`, `springyEmojiCursor`, `emojiCursor`, `clockCursor`, `characterCursor`, `textFlag`. For a presentation default, prefer `followingDotCursor` (smallest visual footprint) or `trailingCursor` (slightly more presence on big-screen projectors).

### hexagoncircle/click-spark — `<click-spark>` Web Component (mouse-down feedback)
**Rating:** ★★★★★ — one file, no build step, themable via a single CSS custom property
**Source:** [hexagoncircle/click-spark/click-spark.js](https://github.com/hexagoncircle/click-spark/blob/main/click-spark.js) | MIT
**Use when:** Click feedback should reinforce live interactions — demos, button-led walkthroughs, anything where the speaker is clicking on stage.

```html
<!-- in index.html, just before </body> -->
<script type="module" src="js/vendor/click-spark.js"></script>
<click-spark style="--click-spark-color: var(--accent);"></click-spark>
```

**Notes for the slide deck context:**
- Wrap the spark element so it sits **above** slide iframes but **below** chrome (HUD, progress bar, counter): give it `position: fixed; inset: 0; pointer-events: none; z-index: 5;` and keep the HUD at `z-index: 50`.
- Click events fired inside an iframe are captured by the iframe's document, not the parent — the spark fires on parent-level chrome (overview thumbnails, navigation hints) which is the intended behaviour.
- For a focal-lane variant, use `--click-spark-color: var(--violet)` on a second `<click-spark>` scoped to a specific container.

### Wiring on `index.html` (recommended pattern)
```html
<!-- ── Pointer chrome (parent only) ────────────────────── -->
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

`pointer: coarse` short-circuit opts touch devices out cleanly. `prefers-reduced-motion` short-circuit honours OS settings even though the cursor library handles it internally too. For offline-friendly decks, vendor both files into `js/vendor/` and replace the two URLs with relative paths.

---

### Catppuccin — Warm pastel palette system, 4 flavors
**Rating:** ★★★☆☆ — strong palette for pastel/cozy aesthetics; too niche for general use
**Repo:** [catppuccin/catppuccin](https://github.com/catppuccin/catppuccin) | 18k+ stars
**Use when:** User wants soft, pastel-toned themes. 26 named colors × 4 flavors (Latte → Mocha).
```
Latte (light) · Frappe · Macchiato · Mocha (darkest)
Colors: Rosewater, Flamingo, Pink, Mauve, Red, Peach, Yellow, Green, Teal, Sapphire, Blue, Lavender + neutrals
```

**Mocha (dark) key values — map to CSS variable contract:**
```css
/* Catppuccin Mocha → theme.css */
:root {
  --bg:      #1e1e2e; /* Base */
  --surface: #313244; /* Surface0 */
  --border:  #45475a; /* Surface1 */
  --text:    #cdd6f4; /* Text */
  --muted:   #6c7086; /* Overlay0 */
  --accent:  #cba6f7; /* Mauve — or swap: Sapphire #89b4fa, Peach #fab387 */
}
```

### Nord — Arctic minimal
**Rating:** ★★★☆☆ — iconic muted palette; works for understated tech decks, limited expressive range
**Repo:** [nordtheme/nord](https://github.com/nordtheme/nord) | 6k+ stars
16 colors: Polar Night (dark), Snow Storm (light text), Frost (accent blues), Aurora (semantic).

**Nord → theme.css:**
```css
:root {
  --bg:      #2e3440; /* Polar Night 0 */
  --surface: #3b4252; /* Polar Night 1 */
  --border:  #434c5e; /* Polar Night 2 */
  --text:    #eceff4; /* Snow Storm 2 */
  --muted:   #4c566a; /* Polar Night 3 */
  --accent:  #88c0d0; /* Frost — or #81a1c1 for cooler blue */
}
```

### Coolors Contrast Checker — WCAG AA/AAA verification
**Rating:** ★★★★☆ — fast, visual, covers AA/AAA for normal and large text; free, no sign-in
**URL:** [coolors.co/contrast-checker](https://coolors.co/contrast-checker)
**Use when:** Review phase `06-review.md` requires WCAG AA (4.5:1 normal / 3:1 large text). Paste hex values for text and background, read ratio instantly.

Alternative: [WebAIM Contrast Checker](https://webaim.org/resources/contrastchecker/) — same function, more detail on pass/fail thresholds.

> **Minimum targets for slides:** body text ≥ 4.5:1, heading text (≥24px bold) ≥ 3:1, UI labels ≥ 4.5:1. Dark-on-light and light-on-dark both need checking.

---

### Radix Colors — Perceptually uniform color scales
**Rating:** ★★★★☆ — best tool for building custom surface/muted/border steps from a base hue; perceptually uniform in dark + light
**Repo:** [radix-ui/colors](https://github.com/radix-ui/colors)
**CDN:** `https://cdn.jsdelivr.net/npm/@radix-ui/colors/dist/index.css`
Semantic + decorative color scales. Each hue has 12 steps (1=bg tint, 9=solid accent, 12=text). Automatic P3 wide-gamut.

**Pick a hue, map steps to the CSS variable contract:**
```css
/* Example: Slate (neutral) + Violet (accent) — dark theme */
@import "https://cdn.jsdelivr.net/npm/@radix-ui/colors/dist/dark/slate.css";
@import "https://cdn.jsdelivr.net/npm/@radix-ui/colors/dist/dark/violet.css";

:root {
  --bg:      var(--slate-1);   /* darkest background */
  --surface: var(--slate-3);
  --border:  var(--slate-6);
  --text:    var(--slate-12);
  --muted:   var(--slate-9);
  --accent:  var(--violet-9);  /* solid accent — always WCAG AA on step-1 bg */
}
```

---

## Fonts

### Google Fonts (CDN, no install)
**Rating:** ★★★★★ — largest free library, zero install, reliable CDN; default first stop for every deck
```html
<!-- Dev / technical — JetBrains Mono heading + Inter body -->
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;700&family=Inter:wght@400;500&display=swap" rel="stylesheet">

<!-- Academic / editorial — Lora heading + Inter body -->
<link href="https://fonts.googleapis.com/css2?family=Lora:ital,wght@0,700;1,400&family=Inter:wght@400;500&display=swap" rel="stylesheet">

<!-- Modern technical — Space Grotesk heading + JetBrains Mono body -->
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;700&family=JetBrains+Mono:wght@400&display=swap" rel="stylesheet">

<!-- Warm creative — Instrument Serif heading + Sora body -->
<link href="https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1&family=Sora:wght@400;500&display=swap" rel="stylesheet">

<!-- Premium / keynote — Playfair Display heading + Inter body -->
<link href="https://fonts.googleapis.com/css2?family=Playfair+Display:wght@700&family=Inter:wght@400;500&display=swap" rel="stylesheet">

<!-- Dark bold — Bricolage Grotesque heading + DM Sans body -->
<link href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:wght@400;700&family=DM+Sans:wght@400;500&display=swap" rel="stylesheet">
```
These are starting points — research new pairings via Font Joy or Google Fonts explorer for each deck.

### Fontshare (alternative CDN, open-source)
**Rating:** ★★★★☆ — fewer fonts but higher design quality than Google Fonts average; good for distinctive display faces
```html
<link href="https://api.fontshare.com/v2/css?f[]=satoshi@700,400&f[]=cabinet-grotesk@700&display=swap" rel="stylesheet">
```
Notable: Satoshi, Cabinet Grotesk, Clash Display, General Sans, Switzer.

### Font Joy — ML-powered pairing discovery
**Rating:** ★★★☆☆ — useful research tool, not a CDN; run it during Phase 4 when standard pairings feel wrong
**URL:** [fontjoy.com](https://fontjoy.com/)
**Use when:** Browsing Google Fonts manually isn't surfacing the right pair. Font Joy generates pairings with a "generate" button — lock the heading or body font and regenerate the other. Different job than Fontshare/Google Fonts browsing.

### Type-Scale — Modular type scale calculator
**Rating:** ★★★★☆ — instant visual preview of any base size + ratio; generates the full CSS scale
**URL:** [type-scale.com](https://type-scale.com/)
**Use when:** Choosing heading/body/caption sizes for a new deck; pick a ratio and base, then copy the resulting `font-size` values into `base.css`.

**Recommended scales for 1280×720 slides:**
| Body (base) | Ratio | H1 result | Notes |
|-------------|-------|-----------|-------|
| 24px | 1.333 (Perfect Fourth) | ~57px | Clean, technical |
| 28px | 1.25 (Major Third) | ~55px | Balanced, editorial |
| 26px | 1.414 (√2) | ~74px | Bold, keynote-style |

Plug into CSS variables: `--fs-body: 26px; --fs-h2: 37px; --fs-h1: 52px; --fs-label: 18px;`

---

## Design Research (agent tool guide)

When the user wants a custom aesthetic, the agent SHOULD actively research beyond these defaults:

```
GitHub/Octocode repository search — search "presentation design CSS" or "slide deck aesthetic"
GitHub/Octocode code search — find SKILL.md files with slide design systems
Web search — search "best CSS presentation aesthetics {{current year}}" or "beautiful HTML deck examples"
Web fetch/browser — read codepen.io/trending, dribbble, awwwards for visual inspiration
```

The agent may deviate from the 5 named themes in `design-system.md` if:
- The user describes a specific aesthetic not covered
- Research reveals a clearly better match
- Custom brand colors are provided

When deviating: define a `/* Custom: <name> */` block at the top of `theme.css` and document the creative choices.

---

## Quick selection

| Need | Use |
|------|-----|
| Rich markdown content in slides | marked.js |
| Staggered bullets/cards (modern browsers) | CSS `sibling-index()` + `@starting-style` |
| Staggered bullets/cards (broad support) | Motion `stagger()` |
| Number counters, progress fills | Motion `animate(0, N)` |
| Sequenced multi-element choreography | Motion `timeline()` |
| SVG path animation or complex choreography | GSAP |
| Slide-to-slide transition (with animation) | View Transitions API |
| Real syntax highlighting | highlight.js |
| Bar / line / area / donut / scatter / radar | Chart.js |
| Heatmap / geo / treemap / complex dashboard | ECharts |
| Dense time-series (100+ points, perf-critical) | uPlot |
| Polished multi-type charts with strong defaults | ApexCharts |
| Custom SVG / network / bespoke layout | D3.js |
| Flowcharts, sequence diagrams, architecture | Mermaid.js |
| Soft pastel palette | Catppuccin |
| Arctic minimal palette | Nord |
| Perceptual color scales | Radix Colors |
| Font pairing discovery | Font Joy |
| Type scale (base + ratio → CSS sizes) | type-scale.com |
| WCAG contrast check (AA/AAA) | Coolors contrast checker |
| Export deck to PDF | `npx decktape http://localhost:port deck.pdf` |
| Themed live-presentation cursor | tholman/cursor-effects (`followingDotCursor`) |
| Mouse-down click spark | hexagoncircle/click-spark (`<click-spark>`) |
