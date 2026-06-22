---
name: octocode-brainstorming
description: Idea brainstorming and validation grounded in evidence. Triggers on "brainstorm", "is this worth building", "has anyone built X", "validate my idea", "check if X exists", "research this idea", "what are the prior-art options for Y". Researches GitHub, npm, and the web in parallel, then synthesizes a decision-ready brief — not code or designs.
---

# Octocode Brainstorming — Idea Discovery & Validation

Turn a raw idea into an evidence-grounded brief. Output is a decision-ready brief — never designs, specs, or code.

```text
FRAME → DIVERGE → RESEARCH (parallel) → CROSS-POLLINATE → STRESS-TEST → SYNTHESIZE → DECIDE
```

## Diverge before you converge

Two modes; **mixing them kills both**. Run divergence *first* and *visibly*, then converge hard with evidence.

- **Diverge** — expand framings/options. Defer all judgment; quantity first; combine and build. No "won't work" yet.
- **Converge** — research prior art, stress-test (Advocate vs Critic), weigh evidence, decide.

The first framing the user typed is rarely the best one to research — locking onto it anchors every search. Never critique while generating; never generate while deciding.

**Mode scales divergence to the ask.** State it in one line before diverging; default **Validate** when ambiguous.

| User asks | Mode | Diverge | Converge |
|-----------|------|---------|----------|
| "brainstorm ideas for X", "what could I build in Y" | **Generate** | Heavy — 6–10 angles, then validate the top 2–3 | Validate the shortlist |
| "validate my idea", "is X worth building" | **Validate** | Light — 2–4 reframings so research isn't anchored | Heavy — full research + Advocate/Critic |
| "has anyone built X", "prior-art options for Y" | **Map** | Minimal — adjacent search terms only | Research-led landscape map |

## Operating principles

- **Assume nothing is novel** — find who tried it, where they stopped, and why.
- **Follow the trail** — README → blog → competitor → issues → the hard unsolved problem.
- **Cross-pollinate** — web names a tool → search its repo/pkg; a repo links docs → read them; a complaint about lib X → verify in code. Each surface sharpens the other's queries.
- **Go deep when thin** — read code, issues, PRs, download trends. Shallow matches are starting points, not answers.
- **Synthesize, don't summarize** — original analysis of what the landscape means, not a link list.

## Hard Gates

Stop and ask before passing any. State the situation in 1–2 lines, name options, recommend one. Never continue silently; never ask outside a gate.

1. **Idea too broad** — maps to 3+ unrelated problem spaces. Usually shows in Frame & Diverge when the slate fans into disconnected domains. Stop before research; ask the user to pick a framing or confirm a shallow sweep.
2. **Zero results** — after research, all three surfaces returned <2 meaningful hits each, even post synonym-expansion. Don't run Advocate/Critic; present what you have, flag the gap, ask: narrow / broaden / accept thin evidence.
3. **Contradictory evidence** — crowded on one surface, "unsolved" on another. Don't bury it; surface both sides with citations and ask which signal to weight.
4. **Worker ceiling** — max **5 delegated workers** per session (web slices + Advocate + Critic). If more seem needed, synthesize first and ask for a second pass. No delegation tool → same 5-slot budget, run sequentially.

## Tools

### GitHub & packages — Octocode CLI

Run from repo root: `node packages/octocode/out/octocode.js <command> ... --no-color`. If the native addon won't load, retry with system Node (`/opt/homebrew/bin/node`). Prefer quick commands; use raw tools only for schema-exact fields or bulk — and read `tools <name> --scheme` before any raw `--queries` call.

| Tool | Use for |
|------|---------|
| `pkg` / raw `npmSearch` | npm libraries and source repos |
| `repo` / raw `ghSearchRepos` | Repos by topic, language, stars |
| `ls owner/repo` / raw `ghViewRepoStructure` | How a similar project is organized |
| `grep <kw> owner/repo` / raw `ghSearchCode` | Confirm a concept is actually implemented |
| `cat owner/repo/path` / raw `ghGetFileContent` | Read key files for specific answers |
| `history` / `pr` / raw `ghHistoryResearch` | How similar features shipped in PRs/commits |

Default flow: `repo` (discover) → `pkg` (resolve packages) → `ls`/`grep` (orient) → `cat --mode none` (exact evidence) → `history`/`pr` (change history).

### Web — search → read → follow

Two interchangeable engines in `scripts/` (same CLI: `--query --max-results --time-range --check --help`; same JSON `{engine,answer,results[{title,url,content}]}`):

| Script | Key | Best for |
|--------|-----|----------|
| `serper-search.mjs` | `SERPER_API_KEY` | Fast Google SERP, broad coverage |
| `tavily-search.mjs` | `TAVILY_API_KEY` | AI-curated answers, deep research |

- **Check once at startup:** `node <skill_dir>/scripts/<engine>-search.mjs --check`. Use whichever exits 0; if both, Serper for breadth + Tavily for depth. Both exit 1 → tell user once: add `SERPER_API_KEY` (serper.dev) and/or `TAVILY_API_KEY` (app.tavily.com) to `<absolute skill_dir>/.env`.
- **Loop:** run engine → read best URLs with the runtime web reader (`WebFetch` in Claude; web/open tool or Browser in Codex) → chase leads → repeat to bedrock. Cite final URLs.
- Engine flags: Tavily `--depth basic|advanced`, `--topic general|news`; Serper `--gl`, `--hl`.
- **Worker brief** (per web slice): research <slice> → run engine → read the best **validated** URLs (official docs, technical guides, reputable publications first) → report who's doing it, what's right/wrong, gaps, best URLs with author/date notes; cite all.

**Fallback (no engine):** seed URLs from GitHub READMEs / `awesome-*` lists / package pages, then aggregators (HN, Product Hunt, alternativeto.net, dev.to), then follow leads like engine results. Flag in TL;DR: "Web research limited — no search engine." On 401/403 → key invalid, try the other engine, give the absolute `.env` path; on 429/5xx → switch engine/fallback and continue. Never block on search failure. Never print/commit keys (`.env` is gitignored).

### Smart querying (all surfaces)

- **Semantic expansion** — never search only the user's words; run 2–3 synonyms/reframes in parallel (e.g. "code review" → "pull request analysis", "diff feedback", "static analysis AI"). Seed these from the Frame & Diverge slate.
- **Recency first** — GitHub: ignore repos inactive >2y (prior art, not competition). Web: default `--time-range year`; widen only if <3 results.
- **Quality filter — prefer validated sources.** GitHub: skip forks/skeletons/<10★ unless sole match; prefer recent commits, engaged issues, multiple contributors. Web, in priority order: **official docs & specs → established technical guides → reputable engineering blogs / well-known publications → widely-cited articles → standards bodies & academic/industry papers**, then community discussion (HN/Reddit/StackOverflow) only to corroborate or find leads. Skip SEO spam, AI-content farms, listicles, and undated/anonymous posts. Cite only sources you can attribute (named author/org + date) and that have substantive, verifiable content.

## Workflow

Clarify → Frame & Diverge → Hypothesis map → Parallel research → Cross-pollinate → Advocate vs Critic → Synthesize → Reflect → Present.

**1. Clarify** — one focused question only if ambiguous; else skip.

**2. Frame & Diverge** (defer judgment) — before any tool, expand the idea space with the lenses below. Capture every output, don't filter. Volume by mode (Generate 6–10, Validate 2–4, Map: search terms only).

| Lens | Ask of the idea |
|------|-----------------|
| **Reframe** | What problem is this *really* solving? State it 2–3 ways. |
| **Invert** | What would guarantee it fails / is unnecessary? (→ real risks and moats) |
| **Analogize** | Who solved a structurally similar problem in another domain? |
| **Decompose** | First principles: irreducible parts — which is the hard/novel one? |
| **Combine/shift** | SCAMPER: Substitute, Combine, Adapt, Modify, Put-to-other-use, Eliminate, Reverse. |

Output a compact **framing slate**, then converge once: pick 1–3 framings to research and say why. Judgment is back on. Feed the reframings/analogies into search expansion.

**3. Hypothesis map** — per chosen framing, 4 bullets: **Crowded if / Underserved if / Blocked if / Worth prototyping if**. A plan, not a conclusion; revise as evidence lands.

**4. Parallel research** — hit **all three surfaces**: GitHub + packages (CLI, main agent), and web products / community / adjacent angles (workers, or main agent if no delegation).
- **Cross-pollinate:** web tool name → `repo`+`pkg`; repo link → read it; package README competitors → search both surfaces; web "unsolved" claim → `grep`/`ghSearchCode` to see if anyone solved it in code.
- **CHECKPOINT — before Advocate/Critic:** (1) ≥1 cross-pollination query per surface, received and incorporated; (2) any zero-result surface got ≥1 synonym-expanded retry before being marked failed. Skip cross-pollination only if the worker-ceiling gate fired (note "cross-pollination skipped (budget)").
- **Stop when** one more generic search won't change the verdict, every major claim has a source or `weak` marker, and contradictions are gated or framed as decisions. **Run one more pass when** the weakest major claim lacks a source, both sides lean on the same unverified assumption, or one surface strongly contradicts the others without tripping gate 3.

**5. Advocate vs Critic** (converge) — dispatch both in one message over the **same evidence** (or run as two clearly labeled passes). Record the **decision delta**: what changed, what stayed contested, who had better evidence.
> **ADVOCATE** for "<idea>" — strongest case FOR. Cite repos/packages/web. Bull case only.
> **CRITIC** of "<idea>" — strongest case AGAINST: crowded competitors, abandoned repos, complaints, unsolved problems. Bear case only.

**6. Synthesize** — analyze, don't list. Agree → high-confidence, lead with it. Disagree → decision point, both sides with evidence. Uncountered risk → blocker; unchallenged strength → best direction. Every claim needs a source.

**7. Reflect** (privately) — weakest claim, best contradiction, decision delta, the one cheap search that could flip the verdict, and **whether a set-aside framing now looks stronger**. Act on it if cheap and ungated; else note why in the TL;DR.

**8. Present** — in chat first; scale sections to real content, don't pad. Then offer to save (write only if confirmed): "Save this brief to `.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`?"

```markdown
# Idea: <one-line restatement>

## TL;DR
<Crowded, underserved, or contested? 2–3 sentences. Lead with the framing you researched (and why it beat the literal idea). Note research limits (e.g. no search engine, cross-pollination skipped).>

## Framings Considered
<The slate: 2–10 angles, one line each, marked researched vs set-aside. Headline section in Generate mode.>

## Prior Art — GitHub / Packages / Web
- **<name>** — <what, signal: stars/downloads/positioning>. `<confidence>` <URL>

## Bull Case (Advocate) / Bear Case (Critic)
<Strongest FOR / AGAINST arguments, with evidence.>

## Decision Delta
<What changed after Advocate/Critic and reflection.>

## Verdict
<Agreement, disagreement, key unknowns.>

## Gaps & Opportunities / Risks & Hard Problems
- <item — with source>

## Angles To Pursue
1. **<angle>** — <why>. Closest prior art: <repo/product/package>.

## Recommended Next Step
<e.g. "Prototype the hardest unknown first" / "Too broad — narrow down" / "Ready to build — start with X">
```

**Confidence markers** — every prior-art entry MUST carry one; mark `weak` and note why if unsure.

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
- Contradictions → both sides, weight by recency/authority (gate 3 if it qualifies).
- Zero prior art is usually a red flag, not a moat (gate 2).

## Error recovery

| Situation | Action |
|-----------|--------|
| Octocode CLI / native addon fails | Try system Node path; else continue web-only, flag in TL;DR |
| GitHub rate-limited | Reduce concurrency; continue |
| Search key missing/invalid | Try the other engine → fallback chain; give absolute `.env` path |
| All web tools down | GitHub-only; flag in TL;DR |

Broad / zero-result / contradictory ideas are handled by **Hard Gates 1–3** — stop and ask there.
