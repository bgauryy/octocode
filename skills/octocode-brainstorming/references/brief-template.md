# Idea Brief Template

Use this for the **saved** artifact (`.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`). The chat presentation can be lighter; this is the durable, RFC-like record. Scale sections to real content — delete any that have nothing evidence-backed to say, never pad. This is an **exploratory research brief**, not a spec or an implementation plan: it maps a space and recommends a direction, it does not design the solution.

## Template

```markdown
# Idea Brief: {one-line restatement}

| Field | Value |
|-------|-------|
| **Status** | Exploratory / Validated / Contested / Parked / Too-broad |
| **Mode** | Generate / Validate / Map |
| **Created** | {YYYY-MM-DD} |
| **Verdict** | Crowded / Underserved / Contested / Worth-prototyping |
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

## The Debate
The Advocate and Critic argued the **same evidence** and then rebutted each other.
Record what each claim's reasoning was and whether it survived.

### Bull Case — survived rebuttal
- {claim} — *because* {reasoning}. Evidence: {ref}. Critic's rebuttal: {rebuttal}; **held because** {why}.

### Bear Case — survived rebuttal
- {claim} — *because* {reasoning}. Evidence: {ref}. Advocate's rebuttal: {rebuttal}; **held because** {why}.

### Conceded / Resolved
- {claim that one side dropped after rebuttal, and why} — removes it as a live concern.

### Decision Delta
What changed across the debate: which claims flipped, which stayed contested, who had
the better evidence. This is the value of running the two against each other.

---

## Verdict
The synthesis of the **best surviving arguments from both sides** — not a tie-break, the
strongest defensible position. Agreement → high-confidence, lead with it. Standing
disagreement → name it as a decision point with both sides. Key unknowns called out.

---

## Gaps & Opportunities / Risks & Hard Problems
- {item — with source or `weak` marker}

---

## Angles to Pursue
1. **{angle}** — {why}. Closest prior art: {repo / product / package}.

---

## Recommended Next Step
One action. e.g. "Prototype the hardest unknown first: {X}" / "Too broad — narrow to {Y}" /
"Build — closest base to fork/extend is {Z}" / "Don't build — {existing thing} already covers it".

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
