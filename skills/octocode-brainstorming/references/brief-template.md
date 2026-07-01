# Idea Brief Template

Use this for the **saved** artifact (`.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`). The chat presentation can be lighter; this is the durable, RFC-like record.
Scale sections to real content: keep only sections with evidence-backed content and drop the rest.
The brief is **exploratory research**, not a spec or an implementation plan — it maps a space and recommends a direction rather than designing the solution.

## Template

```markdown
# Idea Brief: {one-line restatement}

| Field | Value |
|-------|-------|
| **Status** | Exploratory / Validated / Contested / Parked / Too-broad |
| **Mode** | Generate / Validate / Map |
| **Created** | {YYYY-MM-DD} |
| **Verdict** | Crowded / Underserved / Contested / Worth-prototyping |
| **Decision** | Build RFC / Prototype First / Narrow / Park / Do Not Build |
| **Research limits** | {none, or: no search engine, cross-pollination skipped (budget), thin evidence} |

---

## TL;DR
Crowded, underserved, or contested? 2–3 sentences. Lead with the framing you actually
researched (and why it beat the literal idea). State the verdict and the single next step.

---

## Framings Considered
The divergence slate: 2–10 angles, one line each, marked `researched` vs `set-aside`.
Headline section in Generate mode; minimal in Validate/Map.

---

## Already in the Workspace
_(Conditional — only when the idea touches the user's own repo. Omit for purely external ideas.)_
What local code already does part of this (`file:line`), and whether the idea means
**building on** it or **replacing** it. This frames everything below.

---

## Landscape — Prior Art
Group by surface. Every entry carries a confidence marker (`strong`/`moderate`/`weak`).

### GitHub
- **{name}** — {what it is; signal: stars / activity / positioning}. `{confidence}` {URL}

### Packages (npm)
- **{name}** — {what}; health: {downloads/wk · last publish · maintainers · open-issue ratio}. `{confidence}` {URL}

### Web / Products
- **{name}** — {what; who's behind it}. `{confidence}` {URL}

---

## Perspective Review
Critical Architect, Visionary Entrepreneur, and Product reviewed the **same evidence**.
Record only what survived challenge, what was conceded, and what remains contested.

### Critical Architect
- {held/contested claim} — *because* {reasoning}. Evidence: {ref}. Decision impact: {risk / blocker / implementation unknown}. `{confidence}`

### Visionary Entrepreneur
- {held/contested claim} — *because* {reasoning}. Evidence: {ref}. Decision impact: {wedge / upside / timing / distribution}. `{confidence}`

### Product
- {held/contested claim} — *because* {reasoning}. Evidence: {ref}. Decision impact: {MVP boundary / adoption friction / success signal}. `{confidence}`

### Conceded / Resolved
- {claim the panel dropped after challenge, and why} — removes it as a live concern.

### Decision Delta
What changed across the review: which claims flipped, which stayed contested, which
perspective had the better evidence, and whether the decision changed.

---

## Verdict
The synthesis of the **best surviving arguments from the panel** — not a vote, the
strongest defensible position. Agreement → high-confidence, lead with it. Standing
disagreement → name it as a decision point with evidence. Key unknowns called out.

---

## Gaps & Opportunities / Risks & Hard Problems
- {item — with source or `weak` marker}

---

## Angles to Pursue
1. **{angle}** — {why}. Closest prior art: {repo / product / package}.

---

## Recommended Next Step
One action. e.g. "Prototype the hardest unknown first: {X}" / "Too broad — narrow to {Y}" /
"Build RFC — hand off to `octocode/octocode-rfc-generator` with packet below" /
"Don't build — {existing thing} already covers it".

---

## RFC Handoff Packet
_(Conditional — include only when Decision is Build RFC or the user explicitly asks for an RFC.)_
- **Problem:** {specific user + painful situation + desired outcome}
- **Chosen framing:** {researched framing}
- **Value thesis:** {who gets what valuable outcome}
- **Surviving evidence:** {links / file anchors}
- **Alternatives to compare:** {do nothing / package / build / hybrid}
- **Constraints and risks:** {architecture, security, product, rollout}
- **Bounded MVP / first slice:** {scope}
- **Open questions:** {what `octocode-rfc-generator` should decide}
- **Success signal:** {metric or observable outcome}

---

## Open Questions
- [ ] {the one cheap search or experiment that could still flip the verdict}

---

## Resources
Every entry must state **how it supports a claim above** — a bare link is not evidence.

### Code references
- [`path/to/file.ts:42`]({repo-or-local-url}) — {what it shows; which claim it backs, e.g. "proves §Already-in-Workspace that we already parse this"}

### Packages
- [{name}]({npm-url}) — {downloads/wk, last publish, maintainers}; backs §Landscape claim that {…}. `{confidence}`

### Web
- [{title}]({URL}) — {named author/org + date}; backs §{section} claim that {…}. `{confidence}`
```
