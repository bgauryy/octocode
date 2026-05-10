# Slide Rules — Best Practices for Thinking Models

Guidance for the `octocode-slides` skill. Each rule of thumb has a rationale and is grounded in design theory, visual communication research, or empirical practice (Duarte, Reynolds, Tufte, Bringhurst, WCAG, NNGroup).

Read this before Phase 3 (Outline), Phase 4 (Design), and Phase 5 (Implementation). Treat these as strong defaults; override them when the user, source material, audience, or format clearly calls for it, and record the reason.

---

## 0. Audience & Depth

Before a single slide is designed, answer two questions: **Who is in the room?** and **How deep do they need to go?** Every other choice is filtered through these answers.

### 0.1 Audience Profiling
Map the audience on two axes before starting the outline:

| Signal | What to extract |
|--------|----------------|
| **Domain expertise** | Expert / Practitioner / Informed / General |
| **Decision role** | Approver · Influencer · Implementer · Observer |
| **Emotional posture** | Skeptical · Neutral · Already bought in |
| **Time pressure** | 5-min briefing · 20-min pitch · 60-min deep-dive |
| **Context mode** | Live presentation · Async / self-read · Hybrid |

The audience profile governs everything: vocabulary, evidence type, depth, chart complexity, and pacing.

### 0.2 Depth Levels

| Level | Description | Slide style |
|-------|-------------|-------------|
| **Executive** | Decision-makers; no time for mechanics. They need the *so what*, the risk, and the ask. | Short decks (≤10 slides). Lead with the conclusion. Stats and `closing` types dominate. No code. No implementation details. |
| **Management** | Need context, trade-offs, and progress. They care about feasibility, not implementation. | Medium decks (10–20 slides). Evidence-forward. `chart`, `two-col`, `comparison` types. High-level architecture ok. |
| **Technical** | Engineers, architects, data scientists. They distrust vague claims and need proof. | Longer decks (15–30+ slides). `code`, `chart`, `stats`, `timeline` types. Specific numbers. Real examples. Diagrams over metaphors. |
| **Mixed** | Multiple expertise levels in the same room. | Design for the least-informed on core claims; put technical depth in appendix slides. Lead with narrative, offer proof on request. |
| **Async / self-read** | No speaker. The deck carries 100% of the meaning. | Denser content is acceptable. Titles should be self-explanatory. Every chart includes its interpretation. Speaker notes become the narration. |

### 0.3 Calibrate Vocabulary to Expertise
- **Expert audience**: use precise domain terms without definition. Avoid oversimplification — it signals disrespect.
- **General audience**: define every term on first use. Use analogies. Avoid acronyms unless universally known.
- **Mixed audience**: define in the title ("Cache Hit Rate — the % of requests served without going to the database") and use the precise term thereafter.

### 0.4 Evidence Type Follows Audience Role
| Audience role | Most persuasive evidence |
|---------------|--------------------------|
| Approver / exec | Business outcomes, risk, ROI, competitor moves |
| Technical implementer | Working code, benchmarks, architecture diagrams, failure modes |
| Skeptic | Third-party data, before/after comparison, reproducible results |
| Already bought in | What's next, what they need to do, timeline |

### 0.5 Depth Determines Slide Count
Avoid padding a deck to look thorough or compressing it to look concise. The right length is the minimum needed to answer the audience's core question at their depth level.

| Depth level | Target slide count |
|-------------|-------------------|
| Executive brief | 5–10 |
| Decision pitch | 8–15 |
| Technical deep-dive | 15–30 |
| Workshop / teaching | 30–60 |

Appendix slides do not count against these limits — they exist for Q&A and reference.

---

## 1. Content Rules

### 1.1 The 1-1-1 Rule
**One idea. One supporting visual. One slide.**
If you cannot state what a slide communicates in a single sentence, split it into two slides. Every element on the slide should serve that single sentence.

### 1.2 Claim Titles, Not Topic Labels
Most slide titles (except `title`, `agenda`, `section`, `closing`) should be **claim sentences** — complete assertions the audience can repeat without seeing the slide again.

| ❌ Topic label | ✅ Claim sentence |
|----------------|------------------|
| "Performance" | "API latency dropped 40% after caching" |
| "Our Team" | "Five engineers shipped this in eight weeks" |
| "Key Findings" | "Users abandon at step 3 — always" |
| "Revenue" | "Q3 revenue grew 18% YoY, driven by enterprise" |

Chart titles follow the same rule: the title IS the insight, not a generic label.

### 1.3 Kill the Bullets
Avoid default bullet stacks. If bullets are necessary: **3–5 words per line, maximum 4 lines per slide**. Each bullet should support the title claim directly. If a bullet needs its own explanation, it is probably a new slide.

### 1.4 The "So What?" Filter
For every piece of data or content, ask: *"So what? Does this directly support the slide's single claim?"* If not, move it to an appendix slide, speaker notes, or cut entirely.

### 1.5 The 40% Text Cut
Dense slides routinely carry ~40% more text than necessary. Before finalizing, remove ~40% of words: tighten bullets, convert paragraph statements into titles, and turn data tables into charts.

### 1.6 Six-Line Target
Maximum **6 lines of text** per slide (including bullets). If the content exceeds this, it belongs on two slides or in speaker notes.

### 1.7 Data Needs Interpretation
Numbers alone are insufficient. Each data slide (chart, stats) should pair the number with its meaning: *What changed? Why does it matter? What should the audience do?* The interpretation is often the title claim.

### 1.8 Cite Your Sources
Charts and images need short source attribution. Even one-liners at caption size (`--t-small`) are sufficient. Uncited statistics erode trust.

---

## 2. Visual / Design Rules

### 2.1 The 3-Second Glance Test
A well-designed slide communicates its main point **before the presenter speaks** — within 3 seconds of appearing. Test: flip to a slide and look away. If you cannot state the point, the layout or content is wrong.

### 2.2 Visual Hierarchy Is Mandatory
Each slide should have a **clear reading order**: what the eye should see first, second, and third. Levers to set hierarchy (in priority order):
1. **Size** — largest element = most important
2. **Weight** — bold for priority text
3. **Color / contrast** — accent color on the single most important element
4. **Position** — top-left gets read first (F-pattern); center-stage for hero layouts (Z-pattern)
5. **Whitespace** — breathing room around the most important element signals its importance

### 2.3 One Accent Per Slide
Use the accent color on the primary focal element — either the heading or one highlighted figure. More than 3 accent-colored elements per slide usually breaks hierarchy.

### 2.4 The 60-30-10 Color Rule
60% of slide area = background (`--bg`). 30% = text and supporting surfaces (`--text`, `--surface`). 10% = accent (`--accent`). This creates visual balance without crowding.

### 2.5 Maximum 3 Colors Per Slide
`--bg`, `--text`, `--accent`. Muted values are derived from the background, not a fourth color. More than 3 distinct visible colors per slide = visual noise.

### 2.6 Contrast Is Non-Negotiable
- `--text` / `--bg` contrast: **≥ 4.5:1 (WCAG AA)**. Target **7:1 (WCAG AAA)** for display text.
- `--accent` / `--bg`: ≥ 4.5:1.
- Gold / yellow accents only work on **dark backgrounds** — they fail AA on light.
- Validate every new color pair at `https://webaim.org/resources/contrastchecker`.

### 2.7 Avoid Encoding Meaning by Color Alone
Color-blind viewers (8% of men) may not see red/green distinction. Add a secondary signal: shape, label, pattern, or icon.

### 2.8 Typography: Two Fonts Maximum
One font for headings, one for body. Monospace for code. The heading font should have **character** — something deliberately chosen, not the system default. The body font should be **highly legible at 18–22pt on screen**.

### 2.9 Type Size Floor
- **Body text**: minimum `--t-body` ≈ **18–24pt**. If content does not fit at this size, the slide has too much content.
- **Headings**: minimum `--t-title` ≈ **28–44pt**.
- **Display** (title/section slides): `--t-display` ≈ **48–64pt**.
- **Captions / metadata**: `--t-small` ≈ **14–18pt**.
- The **10/20/30 Rule** (Kawasaki): 10 slides, 20 minutes, **30pt minimum** font. If you can't fit it at 30pt, you have too much text.

### 2.10 Type Scale on a Modular Ratio
Build font sizes on a ratio of **1.25–1.618 (Golden Ratio)**. This creates natural-feeling jumps between display, title, sub-heading, body, and caption.

| Token | Range | Use |
|-------|-------|-----|
| `--t-display` | 48–64pt | Title slide headline |
| `--t-title` | 28–44pt | Slide heading |
| `--t-sub` | 22–28pt | Sub-heading, key number |
| `--t-body` | 18–24pt | Bullets, paragraphs |
| `--t-small` | 14–18pt | Captions, footnotes |
| `--t-code` | 14–18pt | Code blocks |

### 2.11 Line Height and Letter Spacing
- Display / title: `line-height: 1.12`, `letter-spacing: -0.02em`
- Body: `line-height: 1.6`, `letter-spacing: 0`
- Headings: `line-height: 1.25`
- Line length: **≤ 60 characters** for comfortable reading.

### 2.12 Whitespace Is Emphasis
**≥ 40% of the slide area should be empty**. Whitespace is not wasted space — it is what makes the non-empty space feel important. Density is the enemy of retention.

### 2.13 8pt Spacing Grid
All gaps, paddings, and margins should be multiples of 8px. This creates invisible but felt harmony. Use `--sp-2` (8px), `--sp-4` (16px), `--sp-6` (24px), `--sp-8` (32px) tokens.

### 2.14 Proximity and Grouping (Gestalt)
- Related elements: ≤ 16px gap between them
- Unrelated elements: ≥ 48px gap between them
- Charts: keep title, chart, legend, and source label tightly grouped as one visual unit

### 2.15 One Dominant Visual
Each slide uses **one dominant visual** — a hero image, the main chart, the central diagram. Multiple competing visuals of equal size split attention and communicate nothing.

### 2.16 Image Quality Over Quantity
One high-resolution, purposeful image beats four small clip-art icons. Avoid staged stock photography. Prefer consistent photographic tone and cropping across the deck.

### 2.17 Visual Style Consistency
Choose **one visual mode** across the deck: photography, illustration, or icon sets — not mixed. Icons: consistent style, size, and color weight.

### 2.18 Data-Ink Ratio (Tufte)
**≥ 80% of chart area** should be data-bearing ink. Remove gridlines, borders, unnecessary axes, redundant labels. Chart emphasis = one accent-colored series; all others muted.

### 2.19 Chart Type Follows Purpose
| Purpose | Chart type |
|---------|------------|
| Trend over time | Line |
| Comparing categories | Bar / Column |
| Distribution / proportion | Donut / Pie (max 5 slices) |
| Correlation | Scatter |
| Single dramatic number | Stats (`--t-display`) |

Pie charts: only for simple proportions with ≤ 5 categories. Avoid 3D charts.

For the full visual type → slide type routing (flows, images, timelines, comparisons) and chart library selection per type, see **§4.11** and `references/resources.md` § Data Visualization.

### 2.20 Motion: Subtle or None
Animations should **reveal meaning** — e.g., building a chart series by series. Avoid decorative motion. Prefer one animation style across the deck.

---

## 3. Layout Rules

### 3.1 Two Zones Per Slide
Most slides have two zones: **header** (the claim title) and **body** (supporting evidence). Keep them visually distinct.

### 3.2 Align to One Grid
Text, images, and visual elements should snap to the same invisible grid. Inconsistent alignment reads as careless, regardless of individual quality. Use 3–4 vertical alignment lines and hold them across the deck when practical.

### 3.3 Consistent Padding
Slide edges: `var(--pad)` on all sides, usually not tighter than `var(--sp-6)`. The 5% safe zone rule: keep live content inside a 5% inset from each edge when possible (mirrors broadcast title-safe areas).

### 3.4 Slides Must Not Scroll
If content overflows a single screen: **split into two slides**. Scrolling breaks the visual contract of presentation and disrupts the audience's reading rhythm.

### 3.5 Standard Aspect Ratio
Use **16:9**. It provides the clearest horizontal / vertical hierarchy and works on all modern displays and projectors. Design for 1280×720.

### 3.6 Vary the Layout Rhythm
Avoid repeating the same centered-stack layout across every slide. Alternate between layout types (`content`, `two-col`, `stats`, `quote`, `chart`, `code`, `image`) to maintain visual momentum and signal topic transitions.

### 3.7 Full-Bleed for Transitions
`section`, `closing`, and `quote` slides use full-bleed — vertically centered with generous padding. These act as visual reset moments in the narrative.

### 3.8 Graphic-to-Text Balance
Aim for roughly **3 visual elements per 1 text block** in a balanced deck. Purely text-heavy or purely image-heavy decks both underperform.

---

## 4. Narrative & Structure Rules

### 4.0 Storytelling Fundamentals

The five storytelling principles — **audience is the hero**, **stakes before solution**, **four emotional beats** (Discomfort → Relief → Confidence → Momentum), **specificity is credibility**, **one deliberate surprise**, **no filler slides** — live in `SKILL.md → Storytelling`. They govern all structural and content decisions before any layout or design choice is made.

This document goes deeper on the *mechanics* of each beat: how to write a problem slide that lands, how density should change across the arc, how to land a closing CTA. Read this section after the SKILL.md storytelling section — it adds detail, it doesn't replace.

### 4.1 Pyramid Principle
**Lead with your conclusion, then support it.** Avoid making the audience wait for the big reveal. The first content slide should deliver the main claim. Subsequent slides prove it.

### 4.2 Delivery Arc
Every deck follows three acts:
- **Opening:** Why should the audience care? Hook early with a problem, striking fact, or bold claim.
- **Middle:** Build the case — evidence, data, code, examples. Vary layout types.
- **Close:** Land the insight. Single, clear action.

### 4.3 MECE Principle
Aim for **Mutually Exclusive and Collectively Exhaustive** points: minimal overlap between slides, and together they cover the full argument. Each slide contributes one clear piece of the proof.

### 4.4 The Bridge
Every slide should set up a question that the next slide answers. The title of slide N should create anticipation for slide N+1. This is what makes a deck feel like a conversation, not a data dump.

### 4.5 Contrast in the Middle
Avoid front-loading all problems and then delivering only upside. Maintain credible tension through the middle of the presentation (problem → evidence → challenge → resolution). Contrast keeps the audience engaged.

### 4.6 Visual Metaphors for Abstraction
Use spatial metaphors (funnel, mountain, bridge, roadmap, ladder) to make abstract concepts immediately graspable. A well-chosen metaphor does the conceptual work before a single word is read.

### 4.7 Before/After Framing
For change, migration, tooling, or UX improvements: show the before state, then the after state. The contrast does the selling.

### 4.8 Signposting
Use an agenda slide or a progress indicator when the audience needs orientation. Disoriented audiences stop listening.

### 4.9 One Call to Action
The closing slide should contain **one specific, actionable next step**. Multiple calls to action can cancel each other out. End with an action the audience can execute immediately.

### 4.10 Structure First
Avoid building HTML before the narrative logic is sound. For an AI agent, the outline and ghost-outline test (§5.3) should be complete before Phase 5 begins, with user approval only when the structure is ambiguous or high-stakes. The argument should hold before visual design begins.

### 4.11 Visual Type Follows Content Need

Before assigning any slide type, ask: *"What is the fastest way for this specific audience to grasp this single idea?"* Choose the type that answers that question. Avoid choosing a type for visual variety alone.

| Content has… | Best type | Avoid |
|-------------|-----------|------------|
| A single striking number | `stats` | paragraph describing the number |
| A sequence or process | `timeline` or Mermaid flow | bullet list |
| Two things that differ | `two-col` or `comparison` | dual bullet lists on one slide |
| System architecture / spatial relationships | `image` (real diagram) | text description |
| Working proof — actual code | `code` with highlight.js | describing what the code does |
| Quantitative trend or distribution | `chart` | table of raw numbers |
| A strong external quote | `quote` full-bleed | inline mention |
| A major topic shift | `section` | header on a content slide |
| A before vs. after state | sequential slides or `two-col` | single dense slide |

**Flows and diagrams:** add them when the relationships between components are easier to understand visually than verbally. Validate nodes and edges with sources; if the structure is uncertain, use a placeholder or ask the user.

**Images:** prefer images that show something concrete — a real UI screenshot, an actual architecture diagram, a direct before/after comparison. Avoid stock/mood imagery unless the deck's purpose is explicitly atmospheric or brand-led.

---

## 5. Logical Flow Between Slides

Flow is what turns a collection of slides into an argument. Each slide should earn its position in the sequence. If you removed a slide and the deck still made sense, the slide may be redundant or misplaced.

### 5.1 The Question-Answer Chain
Every slide implicitly **answers the question raised by the previous slide** and **raises the question the next slide answers**. Map the chain before writing titles:

```
Slide 1 (title): [Raises] → "What is the problem?"
Slide 2 (problem): [Answers] "Load time is 8s" → [Raises] "Why?"
Slide 3 (root cause): [Answers] "No caching layer" → [Raises] "What did we do?"
Slide 4 (solution): [Answers] "We added Redis" → [Raises] "Did it work?"
Slide 5 (result): [Answers] "Load time is 0.4s, -95%" → [Raises] "What next?"
Slide 6 (closing): [Answers] "Ship to production by Friday"
```

If a slide answers a question that was never raised, it is out of order. If a slide raises a question that is never answered, it is a dead end.

### 5.2 Transition Signals
Audience disorientation happens silently — they stop following and you don't know until Q&A. Use these signals to maintain orientation:

| Transition type | Signal method |
|-----------------|---------------|
| New major topic | `section` slide (full-bleed, topic name + 1-line teaser) |
| Drill-down into detail | Subtitle references the parent claim ("Why this matters: the cache layer") |
| Return from detail | "Back to the big picture" section slide or explicit bridge in the title |
| Data that proves a claim | Title of chart slide directly references the claim it proves |
| Shift from problem to solution | Explicit `section` slide or a "Before / After" `two-col` slide |
| Shift from past to future | `timeline` slide or explicit time anchor in the title |

### 5.3 The "Ghost Outline" Test
Before writing a single word of content: write out just the titles of every slide in sequence. Read them as a paragraph. The titles alone should tell the complete story — argument, evidence, and conclusion. If they don't, the structure is wrong before the content phase begins.

### 5.4 Dependency Order
Present concepts in the order a new audience needs them. Avoid referencing a concept before introducing it. If slide 7 depends on understanding something from slide 12, reorder or add a bridge slide.

### 5.5 Section Anchoring
For decks > 10 slides, use `section` slides as **structural landmarks**. Every section slide should state:
1. The topic name (the label)
2. Why this section exists (the 1-line teaser — what question it answers)

A section slide says to the audience: *"We're now here in the argument, and here's why."*

### 5.6 Momentum: Vary Density by Position
Slides at the **start** of a section should be lighter — a claim or a question. Slides in the **middle** carry the evidence and complexity. Slides at the **end** of a section synthesize and lead to the next section's question. A deck that is uniformly dense has no rhythm and exhausts the audience.

| Position in section | Recommended density |
|---------------------|---------------------|
| First slide | Low — title + hook or `section` layout |
| Middle slides | High — `chart`, `code`, `two-col`, `stats` |
| Last slide in section | Medium — synthesis or bridge forward |

### 5.7 Give Data Context
Each data slide (`chart`, `stats`, `code`) should be **preceded or followed by a slide that contextualizes it**. Data without context creates confusion. Context without data creates skepticism. The pair is the argument.

### 5.8 Appendix Flow
Appendix slides are NOT part of the main flow. They are labeled as appendix and appear after the `closing` slide. They are prepared for anticipated questions, not delivered by default. Reference them verbally during Q&A if needed.

---

## 6. UX (Audience Experience) Rules

### 6.1 Design for Scanning, Not Reading
Humans skim before they read. Expect the audience to scan for 3 seconds per slide, not read every word linearly. Every design choice should optimize for **comprehension on first glance** — not thoroughness.

### 6.2 The Skim Test
Flip through the entire deck at 2 seconds per slide. It should feel **calm, not chaotic**. If it feels cluttered, dense, or inconsistent in the skim, something is wrong before a word is read.

### 6.3 The Stranger Test
Show the deck to someone unfamiliar with the topic. If they can describe what each slide is about without explanation, the layout works. If they need verbal description, the design failed.

### 6.4 The Swap Test (Anti-Template)
After building the deck: if you swapped the company name / logo with a competitor's and nothing else felt different, the design likely failed. Aim for **at least one visual decision that couldn't come from a default template**.

### 6.5 Miller's Law — Cognitive Chunking
The human working memory holds **7 ± 2 items** (Cowan suggests 3–5). Limit each slide to **3–5 discrete visual chunks** of information. More than 7 = cognitive overload.

### 6.6 Avoid Reading Slides Aloud
Reading every on-screen element signals that one of you is redundant. The slides are visual support for spoken ideas — not a teleprompter. If the slide contains everything you're going to say, the slide has too much text.

### 6.7 Format-Specific Adaptation
- **Live presentation**: optimize for distance and low-light; heavier visual contrast; larger type.
- **Remote / screen-share**: reduce font size ~10–15% from live presentation defaults; test on the actual screen-share output.
- **Self-navigated / async deck**: all context needs to be on the slide; the speaker notes are the narration; consider denser content.

### 6.8 Speaker Notes for Everything Off-Slide
Detailed explanations, caveats, citations, and transition scripts belong in **speaker notes**, not on the slide itself.

---

## 7. Delivery Rules

### 7.1 The 10/20/30 Rule (Kawasaki)
**10 slides, 20 minutes, 30pt minimum font.** If you cannot make your case in 10 slides in 20 minutes, the argument or the audience needs more work, not more slides.

### 7.2 The B-Key Rule
In live presentation, pressing `B` blacks out the screen. Use it whenever you want the audience to focus entirely on you — especially during Q&A, at transitions, and when telling a story without visual support.

### 7.3 Time Your Rehearsal
Run the full deck at minimum once before delivering. Budget ~5 minutes for Q&A in time-boxed slots. Adjust content after the full pass — not before.

### 7.4 Avoid Verbatim Memorization
Memorizing word-for-word means an unexpected question will derail the flow. Know the argument and the transitions — not the script. Each slide title should be the prompt that surfaces the verbal content naturally.

### 7.5 Opening Hook Options
The first non-title slide should earn attention. Useful hooks:
- A striking number or statistic with built-in tension
- A bold contrarian claim
- A short story that mirrors the audience's lived experience
- A rhetorical question that has no comfortable answer
- A dramatic "before" state that demands a resolution

Weak hooks: "Good morning, today I'll be talking about…" and dry agenda-only openings.

### 7.6 Strong Close Options
Avoid ending on Q&A alone. Options:
- **Call to action** — a single concrete next step
- **Memorable quote** with your own angle (not the quote alone)
- **Closing story** — a short narrative bookend to the opening
- **Restatement of the main claim** with the "so what" made explicit

---

## 8. Technical / HTML Rules

*Specific to the `octocode-slides` HTML implementation.*

### 8.1 CSS Variables Only
No hardcoded `color:`, `font-family:`, or `px` sizes in slide HTML. All values come from `var(--token)`. This makes theme changes instant and consistent.

### 8.2 One Slide = One File
Keep multiple slides in separate HTML files. The iframe-based navigation model expects one file per slide. Scrolling within a slide usually means the slide has too much content.

### 8.3 No System Fonts
Every deck uses named Google Fonts or Fontshare fonts explicitly loaded via `<link>` in the slide's `<head>`. System font fallbacks are only for the stack's safe fallback, not the primary choice.

### 8.4 `clamp()` for All Sizes
Use `clamp(min, preferred, max)` for `font-size` and spacing values where responsive behavior matters. Avoid single hardcoded `px` values for type. Use `calc(-1 * clamp(...))` for negative values.

### 8.5 No-Scroll Contract
Each slide file should render entirely within `1280×720` without overflow. `overflow: hidden` on the slide root helps enforce this. If content overflows in implementation, split or simplify the slide.

### 8.6 CDN Libraries Per-Slide Only
Load libraries (Chart.js, ECharts, uPlot, ApexCharts, D3.js, Motion, highlight.js, Mermaid.js, marked.js) in the individual slide HTML that needs them, rather than globally in `index.html`. Each iframe is an isolated document with its own dependency scope.

### 8.7 Animations: Load on `DOMContentLoaded`
Motion.js or CSS animation sequences should fire inside a `DOMContentLoaded` listener. Animations that trigger before the iframe is visible can desync with user navigation.

---

## 8.8 — 8.9 Anti-Hallucination and Content Brevity Rules

### 8.8 Never Invent Content

| What to never invent | Why |
|---------------------|-----|
| Numbers, percentages, statistics | Invented stats destroy trust when the audience checks them |
| Company names, product names, release versions | Confidently wrong names are worse than admitting uncertainty |
| Quotes or attributed statements | Misattributed quotes are uncorrectable after delivery |
| Architecture, system topology, API signatures | "Approximately right" diagrams mislead engineers |
| Timeline dates, project outcomes, team sizes | Decision-makers act on these |

**Rule:** If a fact isn't in the user's source material, a verifiable public source, or a local tool result — mark it `[NEEDS SOURCE]`.

**How to handle:** follow `SKILL.md → Hard constraints #1` — ask the user once, then apply the resolution ladder if no source arrives. Never hold the deck blocked past one unanswered ask.

### 8.9 Brevity Rules — Slides Are Not Documents

A slide is a visual moment, not a paragraph. Apply these cuts before implementation:

| Remove | Replace with |
|--------|-------------|
| Transitional phrases ("In summary…", "As we can see…", "This demonstrates that…") | The actual claim — the title already does the transition work |
| Synonym bullets (three bullets that say the same thing differently) | One bullet with the most specific version |
| Filler connectors ("Furthermore", "Additionally", "It is worth noting that") | Delete. The next bullet already continues the thought |
| Hedging qualifiers ("may potentially", "could possibly", "in some cases") | A specific qualifier ("in 3 of 4 cases") or remove |
| Restating the title in the body | The title IS the claim. The body adds evidence or steps, not paraphrase |
| AI-style prose flourishes ("This revolutionary approach…", "By leveraging…") | Concrete noun + verb: "This cuts latency by 40%" |

**Word budget per slide element:**
- Title (claim sentence): ≤12 words
- Subtitle / byline: ≤8 words  
- Bullet: ≤10 words each, ≤5 bullets total
- Stat slide big number: 1 number + 1 label + 1 source line
- Code slide: only the lines that prove the point — no surrounding scaffolding unless it's the point

---

## 9. Anti-Patterns (Banned by Default)

| # | Anti-pattern | Why it fails |
|---|-------------|-------------|
| 1 | Inter or Roboto as the **only** heading font | Generic; signals no design effort |
| 2 | `background-clip: text` gradient on headings | Overused AI-gen cliché; fails on dark/light boundary |
| 3 | Emoji leading every bullet or section | Infantilizes the content; inconsistent scale on projectors |
| 4 | Every slide uses the same centered-stack layout | Removes layout variety that signals topic transitions |
| 5 | Cyan + magenta + purple / pink on dark background | Neon dashboard cliché; low information signal |
| 6 | Animated glowing `box-shadow` on cards | Visually expensive; associated with low-quality UI |
| 7 | Three-dot window chrome on every code block | Decorative noise; rarely contributes to comprehension |
| 8 | Accent color on more than 3 elements per slide | Destroys hierarchy; all emphasis = no emphasis |
| 9 | Multiple radial gradient blobs as background | Gradient mesh overwhelms content |
| 10 | All-caps on body text | Reduces reading speed by ~14% (Tinker, 1963); signals shouting |
| 11 | 3D charts or perspective transforms on data | Distorts perception of magnitudes; Tufte anti-pattern |
| 12 | Bullet lists that exceed 4 items | Cognitive overload; audience abandons reading |
| 13 | Full paragraphs on slides | This is a document, not a presentation |
| 14 | "Introduction/Overview" as the title slide claim | Every presentation has one; says nothing |
| 15 | Complementary colors for text-on-background | High saturation complements vibrate and are unreadable |
| 16 | Dark navy text on dark blue background | Fails contrast; projected light makes it worse |

---

## 10. Named Formulas & Rules Reference

| Rule | Formula | Author |
|------|---------|--------|
| **10/20/30** | 10 slides, 20 minutes, 30pt font | Guy Kawasaki |
| **1-1-1** | 1 idea, 1 visual, 1 slide | Garr Reynolds / Duarte |
| **Glance Test** | Audience understands the point in ≤ 3 seconds | Duarte |
| **Pyramid Principle** | Conclusion first, support second | Barbara Minto (McKinsey) |
| **MECE** | Mutually Exclusive, Collectively Exhaustive | McKinsey |
| **60-30-10** | 60% bg, 30% text/surface, 10% accent | Itten / design tradition |
| **Data-Ink Ratio** | ≥ 80% of chart area should carry data | Edward Tufte |
| **Miller's Law** | Working memory: 7±2 items (Cowan: 3–5) | George Miller / Nelson Cowan |
| **6x6 Rule** | ≤ 6 bullets, ≤ 6 words each | Microsoft style guides |
| **Ignite Format** | 5 minutes, 20 slides, auto-advance 15s | O'Reilly / Ignite community |
| **Modular Type Scale** | Font sizes on a 1.25–1.618 ratio | Jan Tschichold / Robert Bringhurst |
| **WCAG AA** | Text/bg contrast ≥ 4.5:1 | W3C |
| **8pt Grid** | All spacing multiples of 8px | Material Design / Bryn Jackson |
| **F-Pattern** | Key content top-left; supporting content left-aligned | NNGroup eye-tracking |
| **Z-Pattern** | Title → hero → CTA in a Z-shape | NNGroup eye-tracking |
| **Gestalt Proximity** | Related ≤16px gap; unrelated ≥48px | Gestalt psychology |

---

## Quick Pre-Delivery Checklist

Before every slide is finalized, run this checklist alongside the three-lens + bidirectional check defined in `SKILL.md → Bidirectional Slide Planning`.

- [ ] **3-second test**: can a stranger state the slide's point in 3 seconds?
- [ ] **Title is a claim**: not a label; contains a verb and a specific assertion
- [ ] **One focal idea**: nothing on the slide contradicts or competes with the title
- [ ] **Font floor**: no text below `--t-small` (≈14pt); body at `--t-body` (≈18–24pt)
- [ ] **Contrast**: `--text`/`--bg` ≥ 4.5:1; `--accent`/`--bg` ≥ 4.5:1
- [ ] **Accent count**: ≤ 3 accent-colored elements
- [ ] **Whitespace**: ≥ 40% of slide area is empty
- [ ] **Scroll-free**: all content fits within 1280×720; no overflow
- [ ] **No anti-patterns**: Visual Slop ≤1/8 and Content Slop 0/8
- [ ] **Skim test**: deck feels calm when flipping at 2 seconds per slide
- [ ] **Bridge**: each slide title implies the question the next slide answers
- [ ] **CTA close**: final slide has one specific, actionable next step
