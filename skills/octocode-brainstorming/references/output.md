# Output — chat brief, confidence, evidence rules

Load at Present (Workflow step 8) and whenever assigning a confidence marker during research. Present in chat first; scale sections to real content, don't pad. When the user confirms a save, write the fuller RFC-like brief with `brief-template.md` and offer: "Save this brief to `.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`?"

## Compact chat skeleton

```markdown
# Idea: <one-line restatement>   ·   Verdict: <crowded|underserved|contested|worth-prototyping>

## TL;DR
<2–3 sentences. Lead with the framing you researched (and why it beat the literal idea). Note research limits (no search engine, cross-pollination skipped, debate shortened).>

## Framings Considered
<The slate: 2–10 angles, one line each, marked researched vs set-aside. Headline section in Generate mode.>

## Already in the Workspace
<Only when the idea touches the user's repo: what local code already does part of this (file:line); build-on vs. replace. Omit for purely external ideas.>

## Landscape — Prior Art (GitHub / Packages / Web)
- **<name>** — <what, signal>. `<confidence>` <URL>   <!-- npm entries: note last-publish · maintainers · open-issue ratio, not just downloads -->

## The Debate — what survived rebuttal
- **Bull (held):** <claim — because <reasoning>; evidence>. **Bear (held):** <claim — because <reasoning>; evidence>. **Conceded:** <what either side dropped>.

## Decision Delta
<What flipped / was conceded / stayed contested across the debate, and who had better evidence.>

## Verdict
<Best-of-both: the strongest defensible position. Agreement, standing disagreement, key unknowns.>

## Gaps & Opportunities / Risks & Hard Problems
- <item — with source>

## Angles To Pursue
1. **<angle>** — <why>. Closest prior art: <repo/product/package>.

## Recommended Next Step
<e.g. "Prototype the hardest unknown first" / "Too broad — narrow down" / "Build — fork/extend X" / "Don't build — Y already covers it">
```

## Confidence markers

Every prior-art entry MUST carry one; mark `weak` and note why if unsure.

| Marker | Criteria |
|--------|----------|
| `strong` | Stars >500 OR downloads >10k/wk OR multiple independent sources confirm |
| `moderate` | Stars 50–500 OR downloads 1k–10k/wk OR single credible source |
| `weak` | Stars <50 OR inactive >1y OR marketing copy only, no independent validation |

## Evidence rules

- Cite everything: GitHub → repo URL + file:line + marker; web → URL + author/org + date + marker.
- **Validated sources only for `strong`/`moderate`.** A claim rates `strong`/`moderate` only when backed by an official doc, established technical guide, reputable publication, or corroborating code/data. Unattributed posts, undated pages, SEO/AI-farm content, and forum opinions are `weak` and used for leads, not proof.
- Marketing copy ≠ validation → `weak` regardless of source authority.
- One source is a lead; **a claim is "proven" only when an independent second source or direct code/data confirms it.**
- Contradictions → both sides, weight by recency/authority (Hard Gate 3 if it qualifies).
- Zero prior art is usually a red flag, not a moat (Hard Gate 2).
