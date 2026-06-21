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
- **Web ↔ Code cross-pollination.** Web and GitHub are not separate tracks — they feed each other. A blog post names a tool → search its repo on GitHub. A GitHub repo README links to docs → fetch those docs with the current runtime's web reader. A web discussion complains about library X → Octocode CLI `pkg`/raw `npmSearch` + `grep`/raw `ghSearchCode` to verify. Always use findings from one surface to refine queries on the other.
- **Go deep when results are thin.** Read code, check issue trackers, inspect PRs, check download trends. Shallow matches are starting points.
- **Plan before fanning out.** Name what would prove the idea crowded, underserved, technically blocked, or worth prototyping before searches begin. Revise that map when observations contradict it.
- **Use parallel research aggressively.** Split the idea into facets (technical, market, community, adjacent). Use available delegation tools when the runtime provides them; in Codex, use multi-agent tools if available, otherwise run the slices yourself without dropping any required surface.
- **Force disagreement.** After research, dispatch Advocate (FOR) and Critic (AGAINST) workers or run the two passes yourself with the same evidence. Agreement = high confidence; disagreement = the real decision.
- **Reflect before deciding.** Before presenting, identify the weakest claim, strongest contradiction, and the one search that would most change the verdict. Run that targeted search unless a hard gate or budget ceiling blocks it.
- **Synthesize, don't summarize.** Original analysis of what the landscape means, not just a link list.

---

## Hard Gates

Stop and ask the user before proceeding past any of these. State the situation in 1–2 lines, name the options, and recommend one.

1. **Idea too broad** — the idea maps to 3+ unrelated problem spaces and cannot be meaningfully researched in one pass. Stop after the clarify step, before dispatching any delegated workers. Ask the user to pick one facet or confirm they want a shallow sweep.
2. **Zero results across surfaces** — after the parallel research phase, all three surfaces (GitHub, packages, web) returned <2 meaningful results each even after synonym expansion. Do not proceed to Advocate vs Critic. Present what you found, flag the gap, and ask: narrow the idea, broaden keywords further, or accept thin evidence?
3. **Contradictory evidence** — GitHub/packages show a crowded space but web sources say the problem is unsolved (or vice versa). Do not bury the contradiction in the brief. Stop, surface both sides with citations, and ask the user which signal to weight before synthesizing.
4. **Research worker ceiling reached** — maximum **5 delegated workers** per brainstorm session (web slices + Advocate + Critic combined). If more seem needed, synthesize what you have first and ask whether the user wants a second research pass. If no delegation tool exists, treat the five slots as sequential research slices and keep the same budget.

Do not silently continue past a hard gate. Do not ask outside of gates — gates exist to reduce bad briefs, not to offload decisions.

---

## Tools

### GitHub & packages — Octocode CLI

Use the built CLI from the repo root for code/package/GitHub research:

```bash
node packages/octocode/out/octocode.js <command> ... --no-color
```

If the current runtime's `node` cannot load the native addon, retry with the system Node path (for this repo that is commonly `/opt/homebrew/bin/node`). Prefer quick commands for ordinary research; use raw tools only when you need schema-exact fields or bulk queries. Before any raw `tools <name> --queries` call, read `tools <name> --scheme`.

| Tool | Use for |
|------|---------|
| `pkg` / raw `npmSearch` | npm libraries and source repos |
| `repo` / raw `ghSearchRepos` | Repos by topic, language, stars |
| `ls owner/repo` / raw `ghViewRepoStructure` | How a similar project is organized |
| `grep <keywords> owner/repo` / raw `ghSearchCode` | Confirm a concept is actually implemented |
| `cat owner/repo/path` / raw `ghGetFileContent` | Read key files for specific answers |
| `history` / `pr` / raw `ghHistoryResearch` | How similar features were shipped in PRs/commits |

Good default CLI flow:
1. `repo "<idea terms>" --no-color` to discover candidates.
2. `pkg "<package/library terms>" --no-color` to resolve package/source links.
3. `ls <owner/repo> --no-color` and `grep "<concept>" <owner/repo> --no-color` to orient.
4. `cat <owner/repo/path> --mode none --no-color` for exact evidence.
5. `history <owner/repo[/path]> --no-color` or `pr <owner/repo#number> --no-color` for change history.

**Smart querying:**
- **Semantic expansion** — don't search only the user's exact words. Generate 2–3 synonym/related queries (e.g. "code review" → also "pull request analysis", "diff feedback", "static analysis AI"). Run them in parallel.
- **Recency first** — sort by recently updated/pushed. Ignore repos inactive >2 years unless the user asks for historical context. Stale repos are prior art, not competition.
- **Quality filter** — skip forks, skeleton/tutorial repos, and <10-star repos unless they're the only match. Prefer repos with recent commits, open issues with engagement, and multiple contributors.

### Web — search scripts + runtime web reader

Two layers: **search** (find URLs via Tavily) → **read** (runtime web reader) → **follow** (chase leads). Use all three every time. In Claude-style runtimes, the reader may be `WebFetch`; in Codex, use the available web/search/open tool or Browser plugin for pages that need inspection. Cite final web URLs either way.

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
- **Quality filter** — prioritize: official docs > technical blog posts > HN/Reddit discussions > general articles. Skip SEO spam, listicles, and paywalled pages. When reading pages, verify the page has substantive content before citing it.

**Research loop:** run Tavily → read best URLs with the runtime web reader (quality over quantity) → follow leads in fetched pages → repeat until bedrock.

**Delegated workers:** when a delegation tool exists, spawn one worker per independent web slice. Each runs Tavily + the runtime web reader. Dispatch multiple workers in one message when possible. In Codex, use multi-agent tools when available; if not, run the same web slices yourself and state that delegation was unavailable.

**Worker template:**
> Research <slice> for "<idea>".
> 1. Run `node <skill_dir>/scripts/tavily-search.mjs --query "<q>" --depth advanced --max-results 8`
> 2. Read the best URLs with the runtime web reader.
> 3. Report: who's doing this, what they got right/wrong, gaps, best URLs with notes. Cite all sources.

### Tavily key setup

Script auto-loads `<skill_dir>/.env`. Set up: `cp <skill_dir>/.env.example <skill_dir>/.env` and fill in the key. Env vars override `.env`.

**Safety:** Never print/log/commit `TAVILY_API_KEY`. The `.env` is gitignored.

### Tavily-down fallback (web research without Tavily)

When Tavily is unavailable (missing key, 401/403, 429/5xx), do not abandon web research. Use this fallback chain:

1. **Seed URLs from GitHub** — GitHub repo READMEs, `awesome-*` lists, and package pages link to docs, blogs, and competitor products. Read those URLs with the runtime web reader. This is your primary URL source when Tavily is down.
2. **Read well-known aggregators** — try the runtime web reader on curated sources relevant to the idea:

Examples:
   - `https://news.ycombinator.com/` + search path for the topic
   - `https://www.producthunt.com/` for product-level prior art
   - `https://alternativeto.net/` for competitive landscape
   - `https://dev.to/search?q=<topic>` for community discussion
3. **Follow leads** — every fetched page may contain links to deeper sources. Follow them the same way Tavily results are followed.

Fallback produces fewer results than Tavily. Flag in the TL;DR: "Web research limited — Tavily unavailable, results seeded from GitHub links and known aggregators."

**Error reporting:**
- Tavily 401/403 → key invalid. Tell user: update `<absolute_path>/.env`. Switch to fallback chain.
- Tavily 429/5xx → switch to fallback chain. Continue.
- Always print **absolute path** to `.env`. Never block on search failures.

---

## Workflow

Clarify → Hypothesis map → Parallel research → Advocate vs Critic → Synthesize → Reflect → Present.

### 1. Clarify

If ambiguous, ask one focused question. If clear enough to search, skip.

### 2. Hypothesis Map

Before research, write 4 compact bullets: **Crowded if**, **Underserved if**, **Blocked if**, and **Worth prototyping if**. Treat them as a plan, not a conclusion; update them after cross-pollination if observations change the search direction.

### 3. Parallel Research

**Every brainstorm must hit all three surfaces.** Main agent handles GitHub + packages via Octocode CLI; delegated workers handle web slices using Tavily + the runtime web reader. If no delegation is available, the main agent still runs all required web slices.

| Track | Runner | Tools |
|-------|--------|-------|
| GitHub prior-art | Main agent | Octocode CLI `repo` → `ls` → `grep` |
| Package landscape | Main agent | Octocode CLI `pkg` |
| Web — products | Worker or main agent | Tavily → runtime web reader |
| Web — community | Worker or main agent | Tavily → runtime web reader |
| Web — adjacent angles | Worker or main agent | Tavily → runtime web reader |

**Cross-pollination pass:** after the initial parallel sweep, use each surface's findings to sharpen the other:
- Web mentions a tool/library name → Octocode CLI `repo` + `pkg` for it
- GitHub repo links to docs/blog/product page → read it with the runtime web reader
- Package README references competitors → search those on both web and GitHub
- Web discussion names an unsolved problem → Octocode CLI `grep` or raw `ghSearchCode` to see if anyone solved it in code

**CHECKPOINT — do not proceed to Advocate vs Critic until:**
1. At least **one cross-pollination query** has been dispatched per surface (web finding → GitHub search, GitHub finding → runtime web reader, package finding → web or GitHub search).
2. Results from cross-pollination have been received and incorporated.
3. If a surface returned zero useful results, at least one synonym-expanded retry was attempted before marking it failed.

**Stop/continue gate:** proceed when one more generic search is unlikely to change the top verdict, each major claim has a credible source or `weak` marker, and contradictions have either triggered a hard gate or are framed as decision points. Do one targeted extra pass when the weakest major claim lacks a source, both Advocate and Critic rely on the same unverified assumption, or one surface strongly contradicts the others without triggering the contradiction gate.

Skip cross-pollination only if the **Research worker ceiling** gate fires first — in that case, note "cross-pollination skipped (budget)" in the brief.

**Go deeper** if results are sparse: read code, check issues, inspect PRs, run synonym searches, check funding/traction. Spawn additional workers when available (within the 5-worker ceiling); otherwise run the highest-value follow-ups yourself.

**Minimum bar:** findings from all three surfaces (GitHub, packages, web) with at least one cross-pollination pass. Flag explicitly if a track failed.

### 3b. Advocate vs Critic

After research, dispatch **two competing workers** in one message with the same findings when delegation is available. If not, run the Advocate and Critic passes yourself as two separate analyses over the same evidence and label them clearly.

Both sides MUST use the same evidence set. After both passes, record the **decision delta**: what changed, what stayed contested, and which side had better evidence. If neither side changes the verdict, say why.

**Advocate:**
> You are the ADVOCATE for "<idea>". Build the strongest case FOR. Cite repos, packages, web sources. Bull case only — not balanced.
> Research findings: <paste>

**Critic:**
> You are the CRITIC of "<idea>". Build the strongest case AGAINST. Cite crowded competitors, abandoned repos, complaints, unsolved problems. Bear case only — not encouraging.
> Research findings: <paste>

### 4. Synthesize

Merge all tracks + Advocate vs Critic. Analyze, don't list.

- Both **agree** → high-confidence signal, lead with these
- They **disagree** → real decision points, present both sides with evidence
- Uncountered risk → flag as blocker. Unchallenged strength → flag as best direction.

Every claim needs a source (repo URL, npm page, web URL). Surface contradictions. Look for: prior art, gaps, risks, angles, traction signals.

### 5. Reflection

Before presenting, answer privately: weakest claim, best contradiction, decision delta, and the one search/read that could still change the recommendation. If that search is cheap and no hard gate blocks it, run it before presenting; if skipped, state why in the TL;DR.

### 6. Present

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

## Decision Delta
<What changed after Advocate/Critic and reflection.>

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
| Octocode CLI unavailable or native addon fails | Try the system Node path, then continue web-only if the CLI still cannot run; flag the limitation in the TL;DR |
| GitHub rate-limited | Reduce concurrency; continue |
| Tavily key missing/invalid | Switch to **Tavily-down fallback** chain; tell user absolute path to `.env` |
| All web tools down | GitHub-only; flag in TL;DR |
| Idea too broad | **Hard gate 1** fires — ask user to narrow before dispatching delegated workers |
| Zero prior art | Synonym-expand and retry once. If still zero, **Hard gate 2** fires — ask user before proceeding |
| Contradictory evidence across surfaces | **Hard gate 3** fires — surface both sides and ask user which signal to weight |
