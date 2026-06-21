# Slide Wireframes

Pick the layout **from the content**, not from aesthetics. Each wireframe below ties three things together:

1. **Content signals** — what the slide actually contains.
2. **Wireframe** — slide regions, proportions, and focal point.
3. **DOM + CSS** — the existing flex/grid primitives in `base.css` that produce the layout.

All wireframes assume the standard shell from `references/html-templates.md`:

```html
<div class="slide slide--{{type}}">
  <!-- regions go here -->
</div>
```

Inside that root, `.slide` is already `display: flex; flex-direction: column`. Layouts below differ only in:
- `.slide--{{type}}` modifier (centering, padding, color treatment)
- which children fill which region
- which inner container is `flex` row, `grid`, or full-bleed `absolute`

---

## Where Things Live

| File | Owns |
|---|---|
| `references/wireframes.md` (this) | Pick a layout from content. Region structure, flex/grid, DOM tree per slide type. |
| `references/html-templates.md` | Full HTML templates per slide type and the canonical 4-region skeleton. |
| `scripts/base.css` | Canonical CSS — copied verbatim to `css/base.css`. Layout primitives, slide-type rules, components, animations, print. |
| `references/design-system.md` | Tokens, typography scale, palettes, slide density limits, title contract, anti-slop, methodology, external sources. |

If a number, color, font, or density rule shows up here, it is a hint — `design-system.md` is canonical.

---

## Canonical Slide Skeleton (reference)

Each wireframe below renders into the same four regions defined in `html-templates.md`:

```text
.slide
├── .slide-logo      optional · brand mark, top-right (absolute)
├── .slide-header    optional · .title + .description
├── .slide-content   REQUIRED · smart flex body
└── .slide-footer    optional · source / page / link
```

**Only `.slide-content` is required.** The wireframes show the body — the smart-flex contents of `.slide-content`. Add `.slide-header` when the slide has a heading; add `.slide-logo` / `.slide-footer` when the deck's posture calls for them. Omit any region the slide doesn't need.

The DOM blocks below sometimes show only the regions a wireframe leans on, to keep examples readable. The full per-type templates in `html-templates.md` are the canonical markup.

---

## Agent Decision Flow

Look at the slide's content first. Walk the table top-down — the first row that matches wins. The wireframes are starting points: if a slide needs a hybrid (e.g., W4 with a small chart, or W3 with a marginal image), build it. The goal is **the layout that serves this slide**, not a perfect match to a labelled pattern.

| Content signal | Pick wireframe | Slide type |
|---|---|---|
| One sentence, no body needed | W2 Big Message | `title`, `section`, `closing` |
| Deck opener with author/date | W1 Title / Hero | `title` |
| Deck section break | W2 Big Message | `section` |
| 2–4 short bullets explaining one claim | W3 Action Title + Bullets | `content` |
| 1–3 KPIs / metrics | W4 Big Number / Stats Grid | `stats` |
| One chart is the proof | W5 Full-Slide Chart | `chart` |
| Image is the proof or the mood | W6 Image-Led (full-bleed) | `image` |
| Text + screenshot/diagram together | W7 Text + Visual Split | `content` (custom) |
| Two parallel options/states | W8 Side-by-Side | `two-col` or `comparison` |
| One quote | W9 Quote | `quote` |
| Ordered steps over time | W10 Timeline | `timeline` |
| Code is the evidence | W11 Code | `code` |
| Audience needs a roadmap | W12 Agenda | `agenda` |
| Recap + next step | W13 Closing | `closing` |

If two rows match, pick the one with the stronger focal point (number > chart > image > text).

---

## Layout Primitives

The wireframes below combine four primitives. They map directly to CSS in `base.css`:

| Primitive | CSS / class | Use when |
|---|---|---|
| **Stacked column** | `.slide-content` default (flex column, `gap: var(--sp-3)`) | Header above, body of one or more blocks stacked |
| **Centered single block** | `.slide-content--center` (or centered slide types) | One number, one sentence, one quote, one image |
| **Two-up grid / N-up grid** | `.slide-content--grid-2`, `.slide-content--grid-3`, or component classes (`.two-col`, `.comparison`, `.stat-grid`) | Two or three parallel content blocks |
| **Side-by-side row** | `.slide-content--row` | Text + visual, label + value, two unequal columns |
| **Full-bleed layer** | `position: absolute; inset: 0` on `.slide--image img`, `.image-ph-bleed`, `.image-overlay` | Image / video / color fills the slide; content overlays it |

`flex: 1` on the body region is what makes charts, columns, and timelines fill the remaining height under `.slide-header`. The modifier classes live on `.slide-content` itself — you can also drop straight to inline `style="grid-template-columns: 2fr 3fr"` when the layout is unique to one slide.

---

## W1 · Title / Hero

**Content signals:** event/eyebrow + main title + subtitle + author/date. No bullets, no chart.

**Wireframe (16:9):**

```text
┌──────────────────────────────────────────────────────────┐
│                                                          │
│                                                          │
│                  EYEBROW · UPPERCASE                     │ ← .eyebrow  (small, accent)
│                                                          │
│              MAIN TITLE — display size                   │ ← .display  (largest)
│              one or two lines, leading-tight             │
│                                                          │
│              Subtitle that narrows the topic             │ ← .subtitle
│                                                          │
│              Author · Date                               │ ← .meta     (small, muted)
│                                                          │
│                                                          │
└──────────────────────────────────────────────────────────┘
        ↑ vertically + horizontally centered
```

**DOM:**

```html
<div class="slide slide--title">
  <p class="eyebrow fade-in">{{Event}}</p>
  <h1 class="display slide-up">{{Main title}}</h1>
  <p class="subtitle slide-up delay-1">{{Subtitle}}</p>
  <p class="meta fade-in delay-2">{{Author}} · {{Date}}</p>
</div>
```

**CSS that does the work:**
- `.slide--title { justify-content: center; text-align: center; }` — centers the column.
- `.slide` is already `flex-direction: column`, so the four children stack with default gap.

**Agent rule:** if the first slide is a title and an image is *not* explicitly provided, do **not** invent one — keep this text-only hero. Only switch to W6 if the brief specifies a hero image.

---

## W2 · Big Message / Section

**Content signals:** one sentence (≤ 12 words) that opens, transitions, or closes a section. Optional eyebrow/section-number.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│                                                          │
│                                                          │
│   01                                                     │ ← .section-num  (small, accent)
│                                                          │
│   ONE BIG SENTENCE, LEFT-ALIGNED OR CENTERED.            │ ← .display
│                                                          │
│   Optional supporting line, muted.                       │ ← .subtitle
│                                                          │
│                                                          │
└──────────────────────────────────────────────────────────┘
        ↑ vertically centered, text-align varies
```

**DOM:**

```html
<div class="slide slide--section">
  <p class="section-num fade-in">{{01}}</p>
  <h2 class="display slide-up">{{Big sentence}}</h2>
  <p class="subtitle slide-up delay-1">{{Supporting line}}</p>
</div>
```

**CSS:**
- `.slide--section { justify-content: center; }` — vertical centering only; text aligns left by default.

**Agent rule:** if you find yourself adding bullets to "fill" this slide, stop — split the bullets onto the next slide.

---

## W3 · Action Title + Bullets (Content)

**Content signals:** one heading that states the takeaway + 2–4 supporting bullets, each ≤ 12 words.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title — a complete thought                       │ ← .slide-header  (flex-shrink: 0)
│  ──────────────────────────────────────────────────────  │
│                                                          │
│  → Point one — short, evidence-shaped                    │
│  → Point two                                             │ ← .bullets       (flex: 1, gap)
│  → Point three                                           │
│  → Point four (max)                                      │
│                                                          │
└──────────────────────────────────────────────────────────┘
       Header zone above, body zone fills the rest
```

**DOM:**

```html
<div class="slide slide--content">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <ul class="bullets">
    <li class="slide-up delay-1">{{Point one}}</li>
    <li class="slide-up delay-2">{{Point two}}</li>
    <li class="slide-up delay-3">{{Point three}}</li>
  </ul>
</div>
```

**CSS:**
- `.slide` flex column → `.slide-header` is `flex-shrink: 0`, `.bullets` (or any child after) takes the remaining height.
- `.bullets` is itself `display: flex; flex-direction: column; gap: var(--sp-3)`.

**Agent rule:** if there are 5+ items, drop the weakest one or split the slide. If there are 0–1 items, switch to W2 Big Message.

---

## W4 · Big Number / Stats Grid

**Content signals:** 1–3 metrics. Each metric has a value + a label. Optional source line.

**Wireframe (1 KPI):**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title — why this number matters                  │ ← .slide-header
│                                                          │
│                                                          │
│                       73%                                │ ← .stat-value (display size)
│                of weekly active users                    │ ← .stat-label
│                                                          │
│                                                          │
│  Based on Q1 2025 telemetry · n = 1,240                  │ ← .stat-caption
└──────────────────────────────────────────────────────────┘
```

**Wireframe (3 KPIs):**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title                                            │
│                                                          │
│  ┌────────────┐   ┌────────────┐   ┌────────────┐        │
│  │   73%      │   │   2.4×     │   │   12 ms    │        │ ← .stat-grid (grid auto-fit)
│  │  active    │   │  faster    │   │  p95 lat.  │        │
│  └────────────┘   └────────────┘   └────────────┘        │
│                                                          │
│  Source / context line                                   │
└──────────────────────────────────────────────────────────┘
```

**DOM:**

```html
<div class="slide slide--stats">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <div class="stat-grid">
    <div class="stat-item pop-in delay-1">
      <span class="stat-value">73%</span>
      <span class="stat-label">{{Label}}</span>
    </div>
    <!-- repeat 1–3 times -->
  </div>
  <p class="stat-caption fade-in delay-4">{{Source}}</p>
</div>
```

**CSS:**
- `.stat-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--sp-6); flex: 1; align-items: center; }` — auto fits 1, 2, or 3 KPIs without changing markup.
- `.stat-item` is itself `flex-direction: column; align-items: center` so value sits over label.

**Agent rule:** never put a 4th KPI here — if the deck has 4+ metrics, pick the most important one for W4 and move the rest to a W5 chart.

---

## W5 · Full-Slide Chart

**Content signals:** trend, comparison across categories, distribution, or any data where shape matters more than a single value.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title — the insight, not the topic               │ ← .slide-header
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │                                                    │  │
│  │                                                    │  │
│  │                CHART AREA                          │  │ ← .bar-chart / canvas
│  │           (≥ 70% of body height)                   │   ← flex: 1
│  │                                                    │  │
│  │                                                    │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  Source / one-line takeaway                              │ ← .chart-insight
└──────────────────────────────────────────────────────────┘
```

**DOM (CSS bar chart, no JS):**

```html
<div class="slide slide--chart">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <div class="bar-chart" role="img" aria-label="{{description}}">
    <div class="bar-row">
      <span class="bar-label">{{A}}</span>
      <div class="bar" style="--pct:80%"><span>{{value}}</span></div>
    </div>
    <!-- repeat -->
  </div>
  <p class="chart-insight fade-in delay-3">{{Takeaway}}</p>
</div>
```

**DOM (canvas chart, e.g. Chart.js):** swap `.bar-chart` for `<canvas id="..." style="flex:1; width:100%;"></canvas>` and load the lib in `<head>`.

**CSS:**
- `.bar-chart { display: flex; flex-direction: column; gap: var(--sp-3); flex: 1; }` — chart takes all leftover height.
- `.bar-row { display: flex; align-items: center; gap: var(--sp-3); }` — label + bar share a row.
- `.bar { flex: 1; }` plus `--pct` custom property drives the fill via `::after`.

**Agent rule:** never put two charts on one slide. If two are needed, use W8 with one chart per column.

---

## W6 · Image-Led (Full-Bleed)

**Content signals:** image is the proof or the mood. Caption is optional and short.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ ░░░░░░  IMAGE / VIDEO / PLACEHOLDER (absolute) ░░░░░░░░░ │ ← position: absolute; inset: 0
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │
│  ░░░░ gradient overlay (transparent → black) ░░░░░░░░░░░ │ ← .image-overlay
│   Big caption on top of overlay                          │ ← .image-caption (absolute, bottom)
│   Supporting line                                        │
└──────────────────────────────────────────────────────────┘
```

**DOM (image ready):**

```html
<div class="slide slide--image">
  <img src="../assets/{{file}}" alt="{{alt}}">
  <div class="image-overlay" aria-hidden="true"></div>
  <div class="image-caption slide-up delay-1">
    <p class="display">{{Headline}}</p>
    <p class="subtitle">{{Supporting line}}</p>
  </div>
</div>
```

**DOM (image missing):** swap `<img>` for the placeholder block:

```html
<div class="image-ph-bleed" aria-label="Image placeholder: {{description}}">
  [ PLACEHOLDER: {{description}} ]
</div>
```

**CSS:**
- `.slide--image { padding: 0; overflow: hidden; }` — kills inner padding so the image truly bleeds.
- `.slide--image img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }` — image fills regardless of aspect ratio.
- `.image-overlay { position: absolute; inset: 0; background: linear-gradient(...); }` — keeps caption legible.
- `.image-caption { position: absolute; bottom: var(--sp-8); left/right: var(--sp-8); }` — caption pinned to bottom.

**Agent rule:** if there's no image and no overlay caption, this becomes an empty grey rectangle — switch to W1 or W2 instead.

---

## W7 · Text + Visual Split (40/60)

**Content signals:** short text claim + one visual that proves it (screenshot, diagram, image, mini-chart).

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title                                            │ ← .slide-header
│                                                          │
│  ┌──────────────┐  ┌──────────────────────────────────┐  │
│  │              │  │                                  │  │
│  │  Short claim │  │                                  │  │
│  │              │  │   IMAGE / DIAGRAM /              │  │ ← grid 2fr / 3fr
│  │  → detail    │  │   PLACEHOLDER                    │  │
│  │  → detail    │  │                                  │  │
│  │  → detail    │  │                                  │  │
│  │              │  │                                  │  │
│  └──────────────┘  └──────────────────────────────────┘  │
│   .col text-side    .col visual-side (larger)            │
└──────────────────────────────────────────────────────────┘
```

**DOM:**

```html
<div class="slide slide--content">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <div class="two-col" style="grid-template-columns: 2fr 3fr;">
    <div class="col slide-up delay-1">
      <p>{{Short claim}}</p>
      <ul>
        <li>{{detail}}</li>
        <li>{{detail}}</li>
      </ul>
    </div>
    <div class="col slide-up delay-2">
      <!-- image ready: -->
      <img src="../assets/{{file}}" alt="{{alt}}" style="width:100%; border-radius: var(--r-md);">
      <!-- OR image missing: -->
      <div class="image-ph" data-expected="{{description}}">
        <div class="image-ph-inner">
          <p class="image-ph-label">PLACEHOLDER: {{description}}</p>
        </div>
      </div>
    </div>
  </div>
</div>
```

**CSS:**
- Reuses `.two-col { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-6); flex: 1; }`.
- Override `grid-template-columns` inline (`2fr 3fr`) when the visual deserves more weight.
- `.col` and `.image-ph` already use `flex: 1` so the visual fills its column.

**Agent rule:** the visual side always carries the weight. If the visual is decorative (not proof of the claim), drop it and use W3 instead.

---

## W8 · Side-by-Side Comparison

**Content signals:** two parallel things to compare — before/after, current/proposed, option A/B, human/agent.

**Wireframe (with divider):**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title — the conclusion of the comparison         │
│                                                          │
│  ┌─────────────────┐ │ ┌─────────────────┐               │
│  │  BEFORE         │ │ │  AFTER          │               │ ← grid 1fr auto 1fr
│  │  (muted color)  │ │ │  (accent color) │               │
│  │  • point        │ │ │  • point        │               │
│  │  • point        │ │ │  • point        │               │
│  │  • point        │ │ │  • point        │               │
│  └─────────────────┘ │ └─────────────────┘               │
│                       ↑                                  │
│                .cmp-divider                              │
│                                                          │
│  Synthesis: what changed, what wins                      │
└──────────────────────────────────────────────────────────┘
```

**DOM (full comparison with divider):**

```html
<div class="slide slide--comparison">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <div class="comparison">
    <div class="cmp-col cmp-before slide-up delay-1">
      <h3>{{Before}}</h3>
      <ul><li>{{point}}</li><li>{{point}}</li></ul>
    </div>
    <div class="cmp-divider fade-in delay-2"></div>
    <div class="cmp-col cmp-after slide-up delay-3">
      <h3>{{After}}</h3>
      <ul><li>{{point}}</li><li>{{point}}</li></ul>
    </div>
  </div>
</div>
```

**DOM (lighter two-col, no divider):** use the `slide--two-col` template from `html-templates.md` — same grid, no `.cmp-*` color treatment.

**CSS:**
- `.comparison { display: grid; grid-template-columns: 1fr auto 1fr; gap: var(--sp-4); flex: 1; }` — the `auto` middle column is the divider.
- `.cmp-before h3 { color: var(--muted); }` and `.cmp-after h3 { color: var(--accent); }` create the "before is dim, after is bright" pattern automatically.

**Agent rule:** keep the two columns balanced — same number of bullets, same label structure. If one side has 5 bullets and the other has 1, the visual lies.

---

## W9 · Quote

**Content signals:** a direct quote (≤ 30 words) + attribution.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│                                                          │
│      "                                                   │ ← .quote-mark (huge, 25% opacity)
│                                                          │
│      Short, memorable quote in italic                    │ ← .quote-text  (subtitle size)
│      that fits on two lines.                             │
│                                                          │
│      — Speaker name, Title                               │ ← .quote-attr (small, muted)
│                                                          │
│                                                          │
└──────────────────────────────────────────────────────────┘
       ↑ vertically centered, generous padding
```

**DOM:**

```html
<div class="slide slide--quote">
  <p class="quote-mark fade-in" aria-hidden="true">"</p>
  <blockquote class="quote-text slide-up">{{Quote}}</blockquote>
  <cite class="quote-attr fade-in delay-2">— {{Name, Title}}</cite>
</div>
```

**CSS:**
- `.slide--quote { justify-content: center; padding: var(--sp-8) calc(var(--sp-8) * 1.5); }` — extra horizontal padding gives breathing room.

**Agent rule:** if the quote does not fit in two lines comfortably, paraphrase and use W3 instead — long quotes never read well from the back of the room.

---

## W10 · Timeline / Process

**Content signals:** 3–5 ordered steps, each with a label (date, phase, name) and one short note.

**Wireframe (vertical):**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title                                            │
│                                                          │
│   ●  2022 Q1   Started internal prototype                │
│   │                                                      │ ← .timeline (vertical line via ::before)
│   ●  2023 Q4   First customer pilot                      │ ← .tl-item (flex row: dot + text)
│   │                                                      │
│   ●  2024 Q2   GA launch, 12 customers                   │
│   │                                                      │
│   ●  2025 Q1   500+ customers, $4M ARR                   │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**DOM:**

```html
<div class="slide slide--timeline">
  <div class="slide-header">
    <h2 class="title fade-in">{{Action title}}</h2>
  </div>
  <ol class="timeline">
    <li class="tl-item slide-up delay-1">
      <span class="tl-dot"></span>
      <div><strong class="tl-label">{{2022 Q1}}</strong> {{Description}}</div>
    </li>
    <!-- repeat 3–5 times -->
  </ol>
</div>
```

**CSS:**
- `.timeline { position: relative; }` + `.timeline::before { position: absolute; left: 0.55rem; top: 0; bottom: 0; width: 2px; }` — the vertical line.
- `.tl-item { display: flex; gap: var(--sp-3); }` — each row is dot + text, side by side.

**Agent rule:** order matters here. If the steps could be re-ordered without changing meaning, use W3 bullets instead.

---

## W11 · Code

**Content signals:** the implementation detail is the proof. Code block ≤ 20 lines, with a one-line takeaway.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│  Action title — what this code proves                    │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │                                                    │  │
│  │  // syntax-highlighted code                        │  │
│  │  function example() { ... }                        │  │ ← .code-block
│  │                                                    │   ← flex: 1
│  │  // ≤ 20 lines, no horizontal scroll               │  │
│  │                                                    │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  Plain-English takeaway                                  │ ← .code-caption
└──────────────────────────────────────────────────────────┘
```

**DOM:** see the `code` template in `html-templates.md`. Key bits:

```html
<pre class="code-block slide-up delay-1"><code class="language-ts">
{{ ≤ 20 lines }}
</code></pre>
<p class="code-caption fade-in delay-2">{{Takeaway}}</p>
<script>hljs.highlightAll();</script>
```

**CSS:**
- `.code-block { flex: 1; overflow: hidden; }` — block fills remaining height and **never scrolls** (split into multiple slides if it overflows).

**Agent rule:** if the code needs to scroll to be read, split it across two slides. If the code is decorative, switch to W3 with a one-line description.

---

## W12 · Agenda

**Content signals:** 3–5 sections in order. Each section has a name + one-line promise.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│  Agenda                                                  │
│                                                          │
│   01    What this is                                     │
│   02    Why now                                          │ ← .agenda-list (flex column)
│   03    How it works                                     │ ← .agenda-list li (flex row)
│   04    What's next                                      │
│                                                          │
└──────────────────────────────────────────────────────────┘
        ↑ numbers are accent-colored, fixed-width
```

**DOM:** see the `agenda` template in `html-templates.md`.

**CSS:**
- `.agenda-list { display: flex; flex-direction: column; gap: var(--sp-3); }`
- `.agenda-list li { display: flex; align-items: center; gap: var(--sp-3); }` — number sits left of label.
- `.num { min-width: 2.5rem; }` — keeps the numbers vertically aligned even with different label lengths.

**Agent rule:** if there are too many sections to fit, group them. Density limits live in `design-system.md`.

---

## W13 · Closing

**Content signals:** final statement + 1–3 takeaways or one CTA + optional links.

**Wireframe:**

```text
┌──────────────────────────────────────────────────────────┐
│                                                          │
│                                                          │
│              Final statement (display)                   │ ← .display
│                                                          │
│              Call to action / next step                  │ ← .subtitle
│                                                          │
│              [link]   [link]   [link]                    │ ← .closing-links (flex row)
│                                                          │
│                                                          │
└──────────────────────────────────────────────────────────┘
        ↑ vertically + horizontally centered
```

**DOM:**

```html
<div class="slide slide--closing">
  <h2 class="display slide-up">{{Final statement}}</h2>
  <p class="subtitle fade-in delay-1">{{Call to action}}</p>
  <div class="closing-links fade-in delay-2">
    <a href="{{URL}}">{{Link}}</a>
  </div>
</div>
```

**CSS:**
- `.slide--closing { justify-content: center; text-align: center; }`
- `.closing-links { display: flex; gap: var(--sp-4); justify-content: center; }`

**Agent rule:** do not introduce new evidence on the closing slide. If a "thank you" is the only message, prefer a W2 with one strong sentence.

---

## Content → Wireframe Worked Examples

Three short examples showing how the agent should reason from a slide's notes to a wireframe choice.

### Example A
> *Notes:* "Octocode reduces time-to-first-answer from 14m to 3m on the auth-service repo, based on internal benchmark."

- One metric is the proof (`14m → 3m`).
- Pick **W4 Big Number** with one stat. Title states the claim. Caption holds the source.

### Example B
> *Notes:* "Show before/after of the dashboard. Before: cluttered, 6 panels, no hierarchy. After: 3 panels, one focal KPI."

- Two parallel states + visual difference matters.
- Pick **W8 Side-by-Side** with screenshots in each column. Mark both screenshots as `PLACEHOLDER` if the user has not provided files.

### Example C
> *Notes:* "Walk through how the agent calls the MCP tools — search, fetch, summarize, cite."

- 4 ordered steps.
- Pick **W10 Timeline**. Each step gets a label (`search`, `fetch`, `summarize`, `cite`) + one note about what the agent does.

---

## Layout Anti-Patterns

These are *layout* failures. Title-writing, density, and visual-aesthetic anti-patterns are in `design-system.md` (Slide Title Contract, Slide Density, Anti-Slop Guide) — do not duplicate them here.

| Anti-pattern | Symptom | Fix |
|---|---|---|
| Filler bullets | Bullets exist to make the slide look balanced | Remove them; switch to W2 or W4 |
| Visual underweight | Chart or image is < 50% of the body | Switch to W5 / W6; move commentary to caption |
| Decorative image | Image does not prove or set mood | Drop the image; switch to W1 / W2 / W3 |
| Lopsided comparison | One column is full, the other half-empty | Drop the comparison; use W3 |
| Region collision | Header and body overlap, or body has no `flex: 1` | Restore `.slide-header` + body region; let body fill |
