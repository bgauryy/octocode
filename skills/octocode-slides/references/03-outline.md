# Phase 3 — Outline

**Role:** Information architect. You turn research into the smallest narrative structure that achieves the user's goal — choosing slide order, types, and content balance for the audience.

**Input:** `.content/brief.md` · `.content/research.md`
**Output:** `.content/outline.md` · `.content/slides/slug.md` per-slide specs

---

## Step 1 · Read inputs

Read all three now (in parallel):
- `.content/brief.md` — audience, goal, tone, slide count, images inventory
- `.content/research.md` — all sourced facts, code, quotes, data
- `references/slide-rules.md` — master rule set for content, narrative, layout, and anti-patterns (required by Global Rule 9)

---

## Step 2 · Calibrate to audience depth

Before picking an arc, translate the brief's audience profile into slide-level constraints using `references/slide-rules.md` §0 (Audience & Depth).

| From brief | Resolve to |
|-----------|------------|
| Audience expertise | Expert / Practitioner / Informed / General |
| Depth level | Executive · Management · Technical · Mixed · Async |
| Evidence type needed | Business outcomes · Code · Before/after · Timeline |
| Target slide count | Cross-check against depth ranges in §0.5 |

Write one sentence: *"Depth: {{level}} — this means {{slide style implications}}."*

This sentence governs every subsequent decision in this phase. Use it to cut material as much as to add material.

---

## Step 3 · Choose the narrative arc

Before writing any slides, decide how the deck should move:

| Arc | When to use |
|-----|-------------|
| **Problem → Solution** | Pitches, proposals, product launches |
| **Context → Insight → Action** | Executive updates, business reviews |
| **Concept → Examples → Practice** | Technical talks, tutorials, onboarding |
| **Before → After → How** | Case studies, retrospectives, migrations |
| **Now → Future → Path** | Strategy decks, roadmaps, vision talks |

Write one sentence naming the arc and why it fits the audience + goal.

---

## Step 4 · Select slide types

Before writing the outline table, map each idea from research to a slide type that delivers it best. The type is a design decision, not a format choice.

| I want to communicate... | Best type |
|--------------------------|-----------|
| A single striking fact or number | `stats` |
| My claim + 2–4 supporting reasons | `content` |
| Two things differ in important ways | `two-col` |
| External credibility — a powerful quote | `quote` |
| Working proof in real code | `code` |
| Quantitative trend or distribution | `chart` |
| A sequence, journey, or roadmap | `timeline` |
| Help the audience decide between options | `comparison` |
| A photo, diagram, or visual that speaks alone | `image` |
| Shifting to the next major topic | `section` |

**Decision rule:** Choose the type that makes the slide's point legible in 3 seconds without the presenter speaking. If a `content` slide only works when explained aloud, it should be `stats`, `chart`, or `code` instead.

**Avoid type monotony.** Three consecutive `content` slides = audience disengagement. Vary types as the content and arc demand.

**For `chart` slides — also pick the library at outline time (not implementation time):**

| Data shape | Library | Add to outline "Key content" column |
|------------|---------|-------------------------------------|
| Bar, line, area, donut, scatter, radar | Chart.js | `chart · Chart.js` |
| Heatmap, geo, treemap, candlestick | ECharts | `chart · ECharts` |
| Dense time-series (100+ pts) | uPlot | `chart · uPlot` |
| Polished multi-type | ApexCharts | `chart · ApexCharts` |
| Custom SVG, network layout, projection, bespoke chart | D3.js | `chart · D3.js` |
| KPI number / counter | `stats` type + Motion | `stats · Motion countup` |
| Flow / architecture / sequence | Mermaid.js diagram | `image` or `content` with Mermaid embed |
| Static comparison ≤6 bars | CSS-only | `chart · CSS bars` |

Deciding the library in the outline prevents mis-matched library loads at Phase 5 and flags `[NEEDS SOURCE]` for any chart whose data has not yet been confirmed.

---

## Step 5 · Draft the outline

Create `.content/outline.md` inside `.octocode/slides/{{slideName}}/`.

```markdown
# Outline: {{Title}}

**Arc:** {{name}} — {{one sentence justification}}

| # | Title (claim sentence) | Type | From research.md | Key content | Flow logic |
|---|------------------------|------|------------------|-------------|------------|
| 01 | {{Deck title}} | title | — | Title, subtitle, presenter name | Raises: "What is this about?" |
| 02 | Agenda | agenda | — | Section list matching arc | Answers: overview · Raises: "Where do we start?" |
| 03 | {{Claim: specific insight or problem}} | content | §{{section}} | {{bullets from research}} | Answers: {{prior Q}} · Raises: {{next Q}} |
| … | … | … | … | … | … |
| N | {{CTA sentence}} | closing | — | Next step, contact, link | Answers: "What do I do now?" |
```

**Slide type options:**
`title` · `agenda` · `section` · `content` · `two-col` · `stats` · `quote` · `code` · `chart` · `image` · `timeline` · `comparison` · `closing`

**Guidelines:**
- Non-structural slide titles (all except `title`, `agenda`, `section`, `closing`) should be **claim sentences** — sentences the audience can repeat without the slide.
- Source columns should point to real sections in `research.md`, `brief.md`, or a user file. If source support is missing, validate with Octocode/local tools or web research when appropriate; if still unresolved, mark `[NEEDS SOURCE]` and ask the user before making it a confident claim.
- Slide count should stay within the range in `brief.md` (calibrated by depth level in Step 2). If the outline exceeds the upper bound by more than 3, trim slides or explicitly note why the added depth is necessary.
- Prefer the fewest slides that answer the audience's core question. If two adjacent slides make the same point, merge or cut.
- Dense content → split the slide rather than shrinking text.
- Avoid 3 consecutive slides of the same type unless the rhythm is intentional.
- The opening should hook early — state the problem, opportunity, or striking fact before detailed solution content.
- The close should land — one clear insight, one action, one next step.
- **Ghost outline test:** Read the titles alone as a paragraph. They should tell the complete story — argument, evidence, and conclusion — without the body content. If they don't, revise the structure.
- **Question-Answer chain (slide-rules.md §5.1):** Each slide title should answer the implicit question raised by the previous slide and raise the question the next slide answers. Add a "Flow logic" column to the outline table if the chain isn't obvious.
- **Data needs context:** each `chart`, `stats`, or `code` slide should have a context slide before or after it that states what the data means.
- **Appendix slides** go after `closing`, are labeled `[APPENDIX]`, and do not count against the target slide count.

---

## Step 5b · Bidirectional validation

After drafting the outline table, run all three checks before Gate 3. Record weak spots inline so they can be fixed before implementation.

**Top-down (row 1 → N):** Does the opening hook create discomfort before slide 3? Does each section follow logically — no unexplained jumps? Does the ghost outline test pass (read only titles aloud — do they tell the complete story)?

**Bottom-up (row N → 1):** Does the closing CTA trace back to the opening problem? Does each slide's claim support its section? Can every `[NEEDS SOURCE]` be resolved, or should the slide be cut, reframed, or sent to the user for validation?

**Per-slide three-lens check** — reason through all three for every slide:

| Lens | Pass condition |
|------|---------------|
| **Content** | Single claim stated. Evidence sourced or explicitly marked as assumption / `[NEEDS SOURCE]`. Nothing cuttable without losing the point. |
| **UX** | Q→A chain intact. Cognitive load fits depth level. If removed, flow still holds. |
| **UI** | Layout type chosen and justified. Dominant visual identified. 3-second test plausible without verbal explanation. No type monotony with adjacent slides. |

Mark any slide failing two or more lenses `[REVISIT]`. Resolve it before Phase 5 or ask the user which direction to take.

---

## Step 5c · Create per-slide specs

For every outline row, create `.content/slides/slug.md`. This is the slide's source-of-truth planning doc. Keep each spec concise but complete enough that Phase 4 can design it and Phase 5 can build it without re-interpreting the story.

```markdown
# {{NN · Slide title}}

## Identity
- **Type:** {{title / content / chart / code / image / ...}}
- **Section / beat:** {{Discomfort / Relief / Confidence / Momentum / appendix}}
- **Status:** {{ready / needs source / needs asset / revisit}}

## Title
{{Final on-slide title. Non-structural slides use a claim sentence.}}

## Description
{{1–2 sentences describing what the audience should understand from this slide.}}

## Reasoning
{{Why this slide exists, why it appears here, what prior question it answers, and what next question it raises.}}

## Content
- **Speaker notes:** {{2–4 sentences}}
- **Body:** {{final bullets, quote, code summary, or caption text}}
- **Source trace:** {{brief.md / research.md section / user file path}}

## Data
{{Exact numbers, labels, units, source, freshness, and any assumptions. Use "None" if not data-backed.}}

## Widgets
{{Interactive or visual components: counter, progress bar, code block, Mermaid diagram, callout, tabs, etc. Use "None" if plain content.}}

## Graphs
{{Chart type, library, data shape, axes/labels, key insight. Use "None" if no graph.}}

## Images
{{Image path or placeholder description, alt text, crop/framing, overlay needs. Use "None" if no image.}}

## UX / UI
{{Layout type, dominant visual, reading order, whitespace/density note, animation, accessibility/contrast considerations.}}
```

**Rules:**
- Avoid duplicating long research excerpts. Link to the source section and copy only the final slide-ready content.
- If a slide needs a graph, image, widget, or data point and the source is missing, validate with Octocode/web or set `Status: needs source` / `needs asset` before Gate 3.
- Design choices belong in `UX / UI`; deck-wide choices still belong in `DESIGN.md`.
- Phase 4 may refine `UX / UI`, `Widgets`, `Graphs`, and `Images`; preserve the slide's `Title`, `Description`, and `Reasoning` unless the outline is updated.

---

## Gate 3 — Smart stop

**Before showing the Gate 3 summary, run the storytelling arc check:**

| Beat | Required slide | Present? |
|------|---------------|----------|
| **Discomfort** | A slide that surfaces a real problem the audience recognises — before any solution | Slide #__ or `[MISSING]` |
| **Relief** | A slide that reframes or names the insight that makes the problem solvable | Slide #__ or `[MISSING]` |
| **Confidence** | A slide (or slides) with real evidence — numbers, code, outcome — that the solution works | Slide #__ or `[MISSING]` |
| **Momentum** | The closing CTA — one specific action the audience can take immediately | Slide #__ or `[MISSING]` |

Any `[MISSING]` beat means the outline is structurally incomplete. Add the missing slide, merge the beat into an adjacent slide, or ask the user whether that beat is out of scope.

---

Show the user when the structure needs approval or the user has not delegated content decisions. If the user said "your call", "just build it", "fast mode", or equivalent, show a compact outline summary and continue unless there is a real content ambiguity.

```
Outline ready for "{{title}}" — {{N}} slides.

Arc: {{name}} ({{reason}})

Story beats: Discomfort (#__) · Relief (#__) · Confidence (#__) · Momentum (#__)

{{Paste full outline table}}

Does this work?
- Add, remove, or reorder slides
- Change any type or content focus
- Adjust the arc

Reply "good" to proceed to design, or give feedback.
```

Update `.content/outline.md` with any changes before proceeding. Stop here only when approval is required or unresolved `[NEEDS SOURCE]` items would affect the deck's claims.
