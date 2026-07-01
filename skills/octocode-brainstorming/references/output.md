# Output — chat brief, decisions, evidence rules

Load at Present (Workflow step 8), assigning confidence, or preparing an RFC handoff. Present in chat first; scale sections to real content.
When the user confirms a save, write the fuller brief with `brief-template.md` and offer: "Save this brief to `.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`?"

## Compact chat skeleton

```markdown
# Idea: <one-line restatement>   ·   Verdict: <crowded|underserved|contested|worth-prototyping>   ·   Decision: <Build RFC|Prototype First|Narrow|Park|Do Not Build>

## TL;DR
<2–3 sentences. Lead with the framing you researched (and why it beat the literal idea). Note research limits (no search engine, cross-pollination skipped, perspective review shortened).>

## Framings Considered
<The slate: 2–10 angles, one line each, marked researched vs set-aside. Headline section in Generate mode.>

## Already in the Workspace
<Only when the idea touches the user's repo: what local code already does part of this (file:line); build-on vs. replace. Omit for purely external ideas.>

## Landscape — Prior Art (GitHub / Packages / Web)
- **<name>** — <what, signal>. `<confidence>` <URL>   <!-- npm entries: note last-publish · maintainers · open-issue ratio, not just downloads -->

## Perspective Review — what survived
- **Critical Architect:** <held/contested claim — because <reasoning>; evidence>.
- **Visionary Entrepreneur:** <held/contested claim — because <reasoning>; evidence>.
- **Product:** <held/contested claim — because <reasoning>; evidence>.
- **Conceded:** <what the panel dropped after challenge>.

## Decision Delta
<What flipped / was conceded / stayed contested across the review, and which perspective had better evidence.>

## Verdict
<Best-of-panel: the strongest defensible position. Agreement, standing disagreement, key unknowns.>

## Gaps & Opportunities / Risks & Hard Problems
- <item — with source>

## Angles To Pursue
1. **<angle>** — <why>. Closest prior art: <repo/product/package>.

## Recommended Next Step
<one action. If Decision is Build RFC, recommend `octocode/octocode-rfc-generator` and include the handoff packet; do not start implementation here.>

## RFC Handoff
<Only when ready or requested: problem, chosen framing, value thesis, surviving evidence links, alternatives, constraints, risks, bounded MVP/first slice, open questions, and success signal. Otherwise say `not ready` and why.>
```

Decision labels: `Build RFC` = hand off to `octocode/octocode-rfc-generator`; `Prototype First` = test one hard unknown; `Narrow` = choose a tighter user/problem/framing; `Park` = evidence/timing is weak; `Do Not Build` = prior art or risks dominate.

## Confidence markers

Every prior-art entry MUST carry one; mark `weak` and note why if unsure.

| Marker | Criteria |
|--------|----------|
| `strong` | Multiple independent validated sources, or direct code/data proof plus strong usage/activity signals |
| `moderate` | One validated source, or credible source plus corroborating repo/package activity |
| `weak` | Popularity alone, marketing/forum copy, stale/inactive project, or no independent validation |

## Evidence rules

- Cite everything: GitHub → repo URL + file:line + marker; web → fetched/opened URL + author/org + date + marker. Search results/snippets are leads, not citations.
- **Validated sources only for `strong`/`moderate`.** A claim rates `strong`/`moderate` only when backed by an official doc/spec/RFC, academic paper/indexed publisher page, established technical guide, reputable publication, or corroborating code/data.
  Treat unattributed posts, undated pages, SEO/AI-farm content, and forum opinions as `weak` leads, not proof.
- Marketing copy ≠ validation → `weak` regardless of source authority.
- One source is a lead; **a claim is "proven" only when an independent second source or direct code/data confirms it.**
- Contradictions → both sides, weight by recency/authority (Hard Gate 3 if it qualifies).
- Zero prior art is usually a red flag, not a moat (Hard Gate 2).
