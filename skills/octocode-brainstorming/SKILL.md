---
name: octocode-brainstorming
description: Idea brainstorming and validation grounded in evidence. Triggers on "brainstorm", "is this worth building", "has anyone built X", "validate my idea", "check if X exists", "research this idea", "what are the prior-art options for Y". Researches GitHub, npm, and the web in parallel, then synthesizes a decision-ready brief — not code or designs.
---

# Octocode Brainstorming — Idea Discovery & Validation

Research-first skill that turns a raw idea into a grounded brief by hitting **every available surface in parallel** — then synthesizes what exists, what's missing, and what's next. No designs, specs, or code.


---

## Researcher Mindset

You are a **technical researcher**, not a search-engine wrapper.

- **Assume nothing is novel.** Find who tried it, where they stopped, and why.
- **Follow the trail.** README → blog → competitor → issues page → hard unsolved problem. Keep pulling threads.
- **Web ↔ Code cross-pollination.** Web and GitHub are not separate tracks — they feed each other. A blog post names a tool → search its repo on GitHub. A GitHub repo README links to docs → `WebFetch` those docs. A web discussion complains about library X → `packageSearch` + `githubSearchCode` for X to verify. Always use findings from one surface to refine queries on the other.
- **Go deep when results are thin.** Read code, check issue trackers, inspect PRs, check download trends. Shallow matches are starting points.
- **Use parallel agents aggressively.** Split the idea into facets (technical, market, community, adjacent) — dispatch a separate `Task` subagent for each in one message.
- **Force disagreement.** After research, dispatch Advocate (FOR) and Critic (AGAINST) subagents with the same evidence. Agreement = high confidence; disagreement = the real decision.
- **Synthesize, don't summarize.** Original analysis of what the landscape means, not just a link list.

---

## Hard Gates

Stop and ask the user before proceeding past any of these. State the situation in 1–2 lines, name the options, and recommend one.

1. **Idea too broad** — the idea maps to 3+ unrelated problem spaces and cannot be meaningfully researched in one pass. Stop after the clarify step, before dispatching any subagents. Ask the user to pick one facet or confirm they want a shallow sweep.
2. **Zero results across surfaces** — after the parallel research phase, all three surfaces (GitHub, packages, web) returned <2 meaningful results each even after synonym expansion. Do not proceed to Advocate vs Critic. Present what you found, flag the gap, and ask: narrow the idea, broaden keywords further, or accept thin evidence?
3. **Contradictory evidence** — GitHub/packages show a crowded space but web sources say the problem is unsolved (or vice versa). Do not bury the contradiction in the brief. Stop, surface both sides with citations, and ask the user which signal to weight before synthesizing.
4. **Subagent ceiling reached** — maximum **5 `Task` subagents** per brainstorm session (web slices + Advocate + Critic combined). If more seem needed, synthesize what you have first and ask whether the user wants a second research pass.

Do not silently continue past a hard gate. Do not ask outside of gates — gates exist to reduce bad briefs, not to offload decisions.

---

## Tools

### GitHub & packages — Octocode MCP

| Tool | Use for |
|------|---------|
| `packageSearch` | npm libraries |
| `githubSearchRepositories` | Repos by topic, language, stars |
| `githubViewRepoStructure` | How a similar project is organized |
| `githubSearchCode` | Confirm a concept is actually implemented |
| `githubGetFileContent` | Read key files for specific answers |
| `githubSearchPullRequests` | How similar features were shipped (deep mode) |

**Smart querying:**
- **Semantic expansion** — don't search only the user's exact words. Generate 2–3 synonym/related queries (e.g. "code review" → also "pull request analysis", "diff feedback", "static analysis AI"). Run them in parallel.
- **Recency first** — sort by recently updated/pushed. Ignore repos inactive >2 years unless the user asks for historical context. Stale repos are prior art, not competition.
- **Quality filter** — skip forks, skeleton/tutorial repos, and <10-star repos unless they're the only match. Prefer repos with recent commits, open issues with engagement, and multiple contributors.

### Web — search scripts + WebFetch

Two layers: **search** (find URLs via Tavily) → **read** (`WebFetch` full content) → **follow** (chase leads). Use all three every time.

**Search script** in `scripts/`:

| Script | Key needed | Best for |
|--------|------------|----------|
| `tavily-search.mjs` | `TAVILY_API_KEY` | AI-curated, deep research mode |

**Startup — check Tavily:**
1. Run `node <skill_dir>/scripts/tavily-search.mjs --check`
2. Exit 0 → ready. Exit 1 → tell user once:
   > Tavily not configured. Add your key to `<absolute_path_to_skill_dir>/.env`: `TAVILY_API_KEY=tvly-YOUR_KEY_HERE` (get one at https://app.tavily.com/)

**Run searches:**
```bash
node <skill_dir>/scripts/tavily-search.mjs --query "<query>" --depth advanced --max-results 8 --time-range year
```

Tavily: `--depth basic|advanced`, `--topic general|news`, `--time-range day|week|month|year`, `--help`.

**Smart querying:**
- **Semantic expansion** — generate 2–3 synonym/reframed queries per search pass (e.g. "AI code review" → also "LLM pull request feedback", "automated diff analysis"). Run them in parallel.
- **Recency first** — default to `--time-range year`. Only widen to all-time if the user asks or the year window returns <3 results.
- **Quality filter** — prioritize: official docs > technical blog posts > HN/Reddit discussions > general articles. Skip SEO spam, listicles, and paywalled pages. When `WebFetch`-ing, verify the page has substantive content before citing it.

**Research loop:** run Tavily → `WebFetch` best URLs (quality over quantity) → follow leads in fetched pages → repeat until bedrock.

**Subagents:** spawn `Task` (subagent_type `generalPurpose`) for independent web slices. Each runs Tavily + `WebFetch`. Dispatch multiple in one message.

**Subagent template:**
> Research <slice> for "<idea>".
> 1. Run `node <skill_dir>/scripts/tavily-search.mjs --query "<q>" --depth advanced --max-results 8`
> 2. `WebFetch` best URLs.
> 3. Report: who's doing this, what they got right/wrong, gaps, best URLs with notes. Cite all sources.

### Tavily key setup

Script auto-loads `<skill_dir>/.env`. Set up: `cp <skill_dir>/.env.example <skill_dir>/.env` and fill in the key. Env vars override `.env`.

**Safety:** Never print/log/commit `TAVILY_API_KEY`. The `.env` is gitignored.

### Tavily-down fallback (web research without Tavily)

When Tavily is unavailable (missing key, 401/403, 429/5xx), do not abandon web research. Use this fallback chain:

1. **Seed URLs from GitHub** — GitHub repo READMEs, `awesome-*` lists, and package pages link to docs, blogs, and competitor products. `WebFetch` those URLs. This is your primary URL source when Tavily is down.
2. **`WebFetch` well-known aggregators** — try `WebFetch` on curated sources relevant to the idea:

Examples:
   - `https://news.ycombinator.com/` + search path for the topic
   - `https://www.producthunt.com/` for product-level prior art
   - `https://alternativeto.net/` for competitive landscape
   - `https://dev.to/search?q=<topic>` for community discussion
3. **Follow leads** — every `WebFetch`-ed page may contain links to deeper sources. Follow them the same way Tavily results are followed.

Fallback produces fewer results than Tavily. Flag in the TL;DR: "Web research limited — Tavily unavailable, results seeded from GitHub links and known aggregators."

**Error reporting:**
- Tavily 401/403 → key invalid. Tell user: update `<absolute_path>/.env`. Switch to fallback chain.
- Tavily 429/5xx → switch to fallback chain. Continue.
- Always print **absolute path** to `.env`. Never block on search failures.

---

## Workflow

Clarify → Parallel research → Advocate vs Critic → Synthesize → Present.

### 1. Clarify

If ambiguous, ask one focused question. If clear enough to search, skip.

### 2. Parallel Research

**Every brainstorm must hit all three surfaces.** Main agent handles GitHub + packages via Octocode MCP; subagents handle web slices using Tavily + `WebFetch`.

| Track | Runner | Tools |
|-------|--------|-------|
| GitHub prior-art | Main agent | `githubSearchRepositories` → `githubViewRepoStructure` → `githubSearchCode` |
| Package landscape | Main agent | `packageSearch` |
| Web — products | Subagent | Tavily → `WebFetch` |
| Web — community | Subagent | Tavily → `WebFetch` |
| Web — adjacent angles | Subagent | Tavily → `WebFetch` |

**Cross-pollination pass:** after the initial parallel sweep, use each surface's findings to sharpen the other:
- Web mentions a tool/library name → `githubSearchRepositories` + `packageSearch` for it
- GitHub repo links to docs/blog/product page → `WebFetch` it
- Package README references competitors → search those on both web and GitHub
- Web discussion names an unsolved problem → `githubSearchCode` to see if anyone solved it in code

**CHECKPOINT — do not proceed to Advocate vs Critic until:**
1. At least **one cross-pollination query** has been dispatched per surface (web finding → GitHub search, GitHub finding → `WebFetch`, package finding → web or GitHub search).
2. Results from cross-pollination have been received and incorporated.
3. If a surface returned zero useful results, at least one synonym-expanded retry was attempted before marking it failed.

Skip cross-pollination only if the **Subagent ceiling** gate fires first — in that case, note "cross-pollination skipped (budget)" in the brief.

**Go deeper** if results are sparse: read code, check issues, inspect PRs, run synonym searches, check funding/traction. Spawn additional subagents (within the 5-subagent ceiling) rather than sequential follow-ups.

**Minimum bar:** findings from all three surfaces (GitHub, packages, web) with at least one cross-pollination pass. Flag explicitly if a track failed.

### 2b. Advocate vs Critic

After research, dispatch **two competing subagents** in one message with the same findings:

**Advocate:**
> You are the ADVOCATE for "<idea>". Build the strongest case FOR. Cite repos, packages, web sources. Bull case only — not balanced.
> Research findings: <paste>

**Critic:**
> You are the CRITIC of "<idea>". Build the strongest case AGAINST. Cite crowded competitors, abandoned repos, complaints, unsolved problems. Bear case only — not encouraging.
> Research findings: <paste>

### 3. Synthesize

Merge all tracks + Advocate vs Critic. Analyze, don't list.

- Both **agree** → high-confidence signal, lead with these
- They **disagree** → real decision points, present both sides with evidence
- Uncountered risk → flag as blocker. Unchallenged strength → flag as best direction.

Every claim needs a source (repo URL, npm page, web URL). Surface contradictions. Look for: prior art, gaps, risks, angles, traction signals.

### 4. Present

```markdown
# Idea: <one-line restatement>

## TL;DR
<Crowded, underserved, or contested? 2–3 sentences. Note any research limitations (e.g. Tavily unavailable, cross-pollination skipped).>

## Prior Art (GitHub)
- **<repo>** — <what, stars, activity>. `<confidence>` <URL>

## Prior Art (Packages)
- **<package>** — <what, downloads, maintenance>. `<confidence>` <URL>

## Prior Art (Web / Products)
- **<product>** — <positioning, pricing>. `<confidence>` <URL>

## Bull Case (Advocate)
<Strongest FOR arguments with evidence.>

## Bear Case (Critic)
<Strongest AGAINST arguments with evidence.>

## Verdict
<Agreement, disagreement, key unknowns.>

## Gaps & Opportunities
- <gap — with source>

## Risks / Known Hard Problems
- <risk — with source>

## Angles To Pursue
1. **<angle>** — <why>. Closest prior art: <repo/product/package>.

## Recommended Next Step
<e.g. "Prototype the hardest unknown first", "Too broad — narrow down", "Ready to build — start with X">
```

**Confidence markers** — every prior-art entry MUST carry one:

| Marker | Meaning | Criteria |
|--------|---------|----------|
| `strong` | Active, validated, high-signal | Stars >500 OR downloads >10k/week OR multiple independent sources confirm |
| `moderate` | Exists and relevant, but incomplete signal | Stars 50–500 OR downloads 1k–10k/week OR single credible source |
| `weak` | Thin evidence, stale, or tangential | Stars <50 OR inactive >1 year OR only marketing copy, no independent validation |

Do not omit the marker. If you cannot assess confidence, mark `weak` and note why.

Scale sections to real content — don't pad.

**Present in chat first.** Then ask:
> Want me to save this brief? I'll write it to `.octocode/brainstorming/<YYYY-MM-DD>-<topic-slug>.md`

Only write if confirmed.


---

## Evidence Rules

- GitHub → repo URL + file:line for code + confidence marker. Web → URL + date + confidence marker.
- Every prior-art claim carries `strong`, `moderate`, or `weak` (see Confidence markers table above).
- Contradictions → surface both sides, pick on recency/authority. If contradiction triggers the **Contradictory evidence** gate, stop and ask.
- Marketing copy ≠ validation — mark it `weak` regardless of source authority.
- Zero prior art is usually a red flag, not a moat. If zero across all surfaces, the **Zero results** gate fires.

---

## Error Recovery

| Situation | Action |
|-----------|--------|
| Octocode MCP not installed | Tell user how to install; continue web-only |
| GitHub rate-limited | Reduce concurrency; continue |
| Tavily key missing/invalid | Switch to **Tavily-down fallback** chain; tell user absolute path to `.env` |
| All web tools down | GitHub-only; flag in TL;DR |
| Idea too broad | **Hard gate 1** fires — ask user to narrow before dispatching subagents |
| Zero prior art | Synonym-expand and retry once. If still zero, **Hard gate 2** fires — ask user before proceeding |
| Contradictory evidence across surfaces | **Hard gate 3** fires — surface both sides and ask user which signal to weight |
