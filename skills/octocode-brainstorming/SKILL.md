---
name: octocode-brainstorming
description: Idea brainstorming and validation grounded in evidence. Triggers on "brainstorm", "is this worth building", "has anyone built X", "validate my idea", "check if X exists", "research this idea", "what are the prior-art options for Y", "should we add X to our app/codebase". Researches the local workspace (when the idea touches it), GitHub, npm, and the web in parallel, then synthesizes a decision-ready brief — not code or designs.
---

# Octocode Brainstorming — Idea Research & Exploration

Explore an idea space and turn a raw idea into an evidence-grounded brief. This is **exploratory research**: map what exists, find the gaps, and pressure-test the idea — across the local workspace (when relevant), GitHub, npm, and the web. Output is a decision-ready brief — never designs, specs, or code. For "how do I build it" hand off to `octocode-rfc-generator`; this skill stops at "is it worth building, and where's the white space."

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
4. **Worker ceiling** — max **5 delegated workers** per session (web slices + the Advocate/Critic debate, which is up to 4 dispatches across two rounds). If more seem needed, synthesize first and ask for a second pass. No delegation tool → same 5-slot budget, run the debate as sequential labeled passes.

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

### Local workspace — orient here first when the idea touches the user's own repo

The quick commands **auto-route**: give `ls`/`grep`/`cat`/`find` a local **path** instead of `owner/repo` and they hit the workspace; add `lsp <file> --type definition|references|callers|hover` for semantics. Raw tools: `localSearchCode`, `localFindFiles`, `localGetFileContent`, `localViewStructure`, `lspGetSemantics`.

| Tool | Use for |
|------|---------|
| `ls <path>` / raw `localViewStructure` | How the workspace is laid out; symbol outline of a file |
| `grep <kw> <path>` / raw `localSearchCode` | Is this concept *already* implemented here? (`--pattern`/`--rule` for AST shape) |
| `find <kw> <path>` / raw `localFindFiles` | Locate files by name/path/content |
| `cat <path> --mode symbols` / raw `localGetFileContent` | Read the exact code — signatures or full |
| `lsp <file> --type …` / raw `lspGetSemantics` | Call sites, callers, references → blast radius of a change |

**When to use:** the idea is grounded in *this* repo — "should we add X to **our** app", "is Y worth building into **our** codebase", "does **our** system already do Z". Then **orient locally before external research**, so you (a) don't recommend reinventing something the repo already has, and (b) frame every prior-art query with the workspace's real stack, libraries, and naming. **Skip entirely** for purely external ideas (market size, landscape, "has anyone built X out there") — local adds nothing there.

Local orient flow: `ls <workspace>` (structure) → `grep`/`find <concept>` (does it exist already?) → `cat --mode symbols` (how it works) → `lsp` (who depends on it / blast radius). Carry the real lib names, framework, and constraints you find into the GitHub/npm/web queries — local findings sharpen external search the same way cross-pollination does.

### Web — search → read → follow

Two interchangeable engines in `scripts/` (same CLI: `--query --max-results --time-range --check --help`; same JSON `{engine,answer,results[{title,url,content}]}`):

| Script | Key | Best for |
|--------|-----|----------|
| `serper-search.mjs` | `SERPER_API_KEY` | Fast Google SERP, broad coverage |
| `tavily-search.mjs` | `TAVILY_API_KEY` | AI-curated answers, deep research |

- **Check once at startup:** `node <skill_dir>/scripts/<engine>-search.mjs --check`. Use whichever exits 0; if both, Serper for breadth + Tavily for depth. `--check` only confirms a key is *present* — an invalid/expired key still exits 0 here and surfaces as a **401 on the first real query** (see Error recovery). Both exit 1 → tell user once: add `SERPER_API_KEY` (serper.dev) and/or `TAVILY_API_KEY` (app.tavily.com) to `<absolute skill_dir>/.env`.
- **Loop:** run engine → read best URLs with the runtime web reader (`WebFetch` in Claude; web/open tool or Browser in Codex) → chase leads → repeat to bedrock. Cite final URLs.
- Engine flags: Tavily `--depth basic|advanced`, `--topic general|news|finance`, `--include-domains`/`--exclude-domains` (comma-separated), `--start-date`/`--end-date` (YYYY-MM-DD), `--auto-parameters`, `--max-results` (0–20); Serper `--gl`, `--hl`, `--time-range`.
- **Worker brief** (per web slice): research <slice> → run engine → read the best **validated** URLs (official docs, technical guides, reputable publications first) → report who's doing it, what's right/wrong, gaps, best URLs with author/date notes; cite all.

**Fallback (no engine):** seed URLs from GitHub READMEs / `awesome-*` lists / package pages, then aggregators (HN, Product Hunt, alternativeto.net, dev.to), then follow leads like engine results. Flag in TL;DR: "Web research limited — no search engine." On 401/403 → key invalid, try the other engine, give the absolute `.env` path; on 429/5xx → switch engine/fallback and continue. Never block on search failure. Never print/commit keys (`.env` is gitignored).

### Smart querying (all surfaces)

- **Semantic expansion** — never search only the user's words; run 2–3 synonyms/reframes in parallel (e.g. "code review" → "pull request analysis", "diff feedback", "static analysis AI"). Seed these from the Frame & Diverge slate.
- **Recency first** — GitHub: ignore repos inactive >2y (prior art, not competition). Web: default `--time-range year`; widen only if <3 results.
- **Quality filter — prefer validated sources.** GitHub: skip forks/skeletons/<10★ unless sole match; prefer recent commits, engaged issues, multiple contributors. **Packages (npm): downloads alone ≠ healthy** — also weigh **last-publish recency, release cadence, maintainer count, open-issue/PR ratio, and dependency freshness**. A high-download but unmaintained (last publish >1–2y, single maintainer, stale deps) package is a *risk to flag*, not validation — and is often the white-space signal (popular but abandoned = opportunity). Read `pkg` output and the source repo's `history`/issues, don't trust the download badge alone. Web, in priority order: **official docs & specs → established technical guides → reputable engineering blogs / well-known publications → widely-cited articles → standards bodies & academic/industry papers**, then community discussion (HN/Reddit/StackOverflow) only to corroborate or find leads. Skip SEO spam, AI-content farms, listicles, and undated/anonymous posts. Cite only sources you can attribute (named author/org + date) and that have substantive, verifiable content. On Tavily, enforce this *mechanically*: `--include-domains` to pin a query to official docs/specs (e.g. `docs.python.org,arxiv.org`), `--exclude-domains` to drop known farm/aggregator hosts — cheaper and cleaner than filtering only after results land.

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
- **Local first (conditional):** if the idea targets the user's own repo/workspace, run the **Local orient flow** (above) *before* the external surfaces — establish what already exists and the real stack, then frame every GitHub/npm/web query with it. Skip for purely external ideas.
- **Cross-pollinate:** web tool name → `repo`+`pkg`; repo link → read it; package README competitors → search both surfaces; web "unsolved" claim → `grep`/`ghSearchCode` to see if anyone solved it in code.
- **CHECKPOINT — before Advocate/Critic:** (1) ≥1 cross-pollination query per surface, received and incorporated; (2) any zero-result surface got ≥1 synonym-expanded retry before being marked failed. Skip cross-pollination only if the worker-ceiling gate fired (note "cross-pollination skipped (budget)").
- **Stop when** one more generic search won't change the verdict, every major claim has a source or `weak` marker, and contradictions are gated or framed as decisions. **Run one more pass when** the weakest major claim lacks a source, both sides lean on the same unverified assumption, or one surface strongly contradicts the others without tripping gate 3.

**5. Advocate vs Critic — a debate, not two monologues** (converge). The goal is **not** to collect a pro list and a con list; it is to make the two agents *reason against each other* so only claims that survive scrutiny reach the verdict — then keep the **best of both**. Every claim must carry its reasoning *and* a citation; an assertion with neither is dropped before it counts.

- **Round 1 — opening cases** (same evidence, dispatch together):
  > **ADVOCATE** for "<idea>" — strongest case FOR. Each claim: assertion → *because* (reasoning) → citation (repo/package/web/local). Bull case only.
  > **CRITIC** of "<idea>" — strongest case AGAINST: crowded competitors, abandoned repos, complaints, unsolved problems. Each claim: assertion → *because* → citation. Bear case only.
- **Round 2 — rebuttal** (each agent receives the *other's* Round-1 case): rebut specific claims with evidence, **concede** what you cannot refute (say so explicitly), and attack the weakest-supported claim. New citations only — no repeating Round 1.
  > **ADVOCATE rebuttal** — answer the Critic's strongest points; concede what holds.
  > **CRITIC rebuttal** — answer the Advocate's strongest points; concede what holds.
- **Referee / best-of-both** (main agent): keep every claim that *survived* rebuttal (→ high-confidence), drop every claim that was *conceded*, and mark every claim that stayed *contested* as a decision point. The verdict is the strongest defensible position assembled from **both** sides — not whoever shouted louder.

Record the **decision delta**: which claims flipped, which were conceded, which stayed contested, and who had the better evidence.

If there's no delegation tool, run the four passes sequentially with the labels above, feeding each agent the prior pass verbatim. **Budget:** the debate is ~4 worker dispatches — count it against the 5-worker ceiling (gate 4); if web slices already spent the budget, run a single rebuttal round (one Advocate-rebuts + one Critic-rebuts) and note "debate shortened (budget)".

**6. Synthesize** — analyze, don't list. Build the verdict from claims that **survived rebuttal** (best-of-both), not from the raw Round-1 lists. Agree → high-confidence, lead with it. Disagree (still contested after rebuttal) → decision point, both sides with evidence. Uncountered risk → blocker; unchallenged strength → best direction. Every claim needs a source.

**7. Reflect** (privately) — weakest claim, best contradiction, decision delta, the one cheap search that could flip the verdict, and **whether a set-aside framing now looks stronger**. Act on it if cheap and ungated; else note why in the TL;DR.

**8. Present** — in chat first; scale sections to real content, don't pad. Use the compact chat skeleton below. When the user confirms a save, write the fuller **RFC-like brief** using `references/brief-template.md` (status table + grouped landscape + the debate + a **Resources** section where every reference states *how* it supports a claim) — don't duplicate that whole template here. Offer: "Save this brief to `.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`?"

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

## Grounding & references

The method here is not improvised — each pillar maps to an established source. Cited so the instructions stay falsifiable.

**Method**
- *Diverge-then-converge, never mixed* — divergent vs. convergent thinking, originated by J.P. Guilford (1950s). [Divergent thinking — Wikipedia](https://en.wikipedia.org/wiki/Divergent_thinking) · [Divergent vs. Convergent Thinking — Interaction Design Foundation](https://www.interaction-design.org/literature/topics/divergent-thinking)
- *Defer judgment / quantity-first while diverging* — Alex Osborn's brainstorming rules, *Applied Imagination* (1953). [Brainstorming — Wikipedia](https://en.wikipedia.org/wiki/Brainstorming)
- *Combine/shift lens (SCAMPER)* — Bob Eberle, *SCAMPER: Games for Imagination Development* (1971), systematizing Osborn's idea-spurring checklist. [SCAMPER — Wikipedia](https://en.wikipedia.org/wiki/SCAMPER)

**Tooling (web engines)** — flags and limits below reflect the live API contracts (verified 2026-06-22):
- Tavily `/search`: `search_depth`, `topic` (general/news/finance), `time_range`, `max_results` (0–20), `include_domains`/`exclude_domains`, `auto_parameters`, `start_date`/`end_date`. [Tavily API reference](https://docs.tavily.com/documentation/api-reference/endpoint/search)
- Serper `/search`: `q`, `gl`, `hl`, `num`, `page`, `tbs` (recency via `qdr:d|w|m|y`), `autocorrect`, `location`. [serper.dev](https://serper.dev/) · [Serper params (LiteLLM)](https://docs.litellm.ai/docs/search/serper)
