# Tools — research surfaces

Load when starting research (step 4) or whenever you need the exact command/flag for a surface. Covers top resources/web engines, GitHub/packages/code (Octocode CLI), the local workspace, and cross-surface query craft.

## Default external loop — resources first

For external validation, prior-art mapping, method claims, or market/technical landscapes, start with top resources before repository/package/code search:

1. Search/read top resources: official docs, papers, standards, canonical industry articles, credible practitioner writeups, and dated announcements.
2. Extract project, repo, package, author, technique, and failure-term leads from those resources.
3. Search GitHub/packages/code from those leads, then exact-read READMEs, source files, issues, PRs, and package metadata.
4. Loop back to web/resources to reconcile contradictions, stale repos, unsupported claims, and missing context.

Skip this order only when the user explicitly asks for local-only/no-web work, web access is unavailable, or the idea is purely about this workspace.
For repo-targeted ideas, orient locally first or in parallel so resource queries use the real stack.

## GitHub, packages, and local code

When a brainstorm needs Octocode-backed GitHub, package, local code, history, artifact, or semantic research, delegate that research to `octocode-research`.

Use `octocode-research` if installed. If it is missing, use https://github.com/bgauryy/octocode/tree/main/skills/octocode-research or install it with:

```bash
npx octocode skill --name octocode-research
```

Ask `octocode-research` for the needed evidence surface, confidence, and citations; then return here for framing, stress-test, and verdict.

**When to use:** the idea is grounded in *this* repo — "should we add X to **our** app", "is Y worth building into **our** codebase", "does **our** system already do Z".
Then ask `octocode-research` to orient locally before external research. Two payoffs:
- you avoid recommending something the repo already has;
- you frame every prior-art query with the workspace's real stack, libraries, and naming.

**Skip entirely** for purely external ideas (market size, landscape, "has anyone built X out there") — local adds nothing there.

Carry the real lib names, framework, and constraints from `octocode-research` into web queries; local findings sharpen external search the same way cross-pollination does.

## Web/top resources — search → read → follow

Two interchangeable engines in `scripts/` (same CLI: `--query --max-results --time-range --check --presence-only --help`; same JSON `{engine,answer,results[{title,url,content}]}`):

- `serper-search.mjs` (`SERPER_API_KEY`) — fast Google SERP, broad coverage
- `tavily-search.mjs` (`TAVILY_API_KEY`) — AI-curated answers, deep research

- **Check once at startup:** `node <skill_dir>/scripts/<engine>-search.mjs --check`. Use whichever exits 0; if both, Serper for breadth + Tavily for depth.
  `--check` runs a live authorization check; add `--presence-only` only when offline.
  Both exit 1 → tell the user once to add `SERPER_API_KEY` (serper.dev) and/or `TAVILY_API_KEY` (app.tavily.com) to `~/.octocode/.env` (macOS/Linux; Windows `%APPDATA%\.octocode\.env`).
  The `~/.octocode/.env` file loads at session start and by the skill scripts — no skill-local `.env` needed.
- **Loop:** run engine → fetch/open the best formal URLs with the runtime web reader (`WebFetch` in Claude; web/open or Browser in Codex) → extract leads → read exact code → reconcile contradictions.
  Search snippets are leads; cite only fetched/opened sources.
- Engine flags: Tavily `--depth basic|advanced|fast|ultra-fast`, `--topic general|news|finance`, `--include-domains`/`--exclude-domains` (comma-separated), `--start-date`/`--end-date` (YYYY-MM-DD), `--auto-parameters`, `--max-results` (0–20); Serper `--gl`, `--hl`, `--time-range`.

## Research workers — dispatchable (RESEARCH/CROSS-POLLINATE)

Run solo for one quick check. Dispatch as separate workers only when the brainstorm already spans multiple surfaces/query slices and budget allows — each dispatched worker counts against the Hard Gate 5-worker ceiling.

Default two-worker close-loop for web-backed brainstorming:
- **Web Search Scout** runs Serper and/or Tavily for one query slice, then returns ranked leads.
- **Source/Code Checker** fetches the best sources, expands to more sources, and uses `octocode-research` when GitHub/packages/local code evidence matters.
- Run them in parallel when possible; otherwise run them sequentially with separate notes. The final synthesis must reconcile both receipts.

> **Web Search Scout** — owns one query slice. Run Tavily and/or Serper (whichever checked out) → fetch/open the best formal/validated URLs.
> Report who's doing it, what's right/wrong, gaps, and the best URLs with author/date notes. Cite everything fetched.

> **Source/Code Checker** — owns verification after the web leads. Fetch/open source material beyond snippets, then delegate GitHub/packages/local code checks to `octocode-research` when needed.
> Report source quality, code evidence, contradictions, and unresolved gaps. Cite exact URLs, package metadata, repo files, or local `file:line`.

> **Trend & Source Scout** — owns momentum/crowdedness/research-recency for one space. Pull 2-3 entries from `references/trend-sources.md` (cross-domain buzz, articles/papers incl. arXiv, competitive landscape, repo/package momentum, AI trend surfaces, or platform validation — whichever fits).
> Fetch and report the star/velocity/recency/citation/ranking signal, not just presence. Cite everything fetched; mark `weak` if uncorroborated.

Run Web Search Scout and Trend & Source Scout in parallel when an idea needs both breadth and momentum. They cover different evidence, not the same query twice.

**Model tier for these two scouts:** fetch-and-summarize is mechanical, not judgment.
When the host lets you pick a worker model (Cursor `Task` `model`; Pi `spawnAgent` `model`), dispatch both scouts on a small/fast model (e.g. `composer-2.5`, Haiku, or the host's fast tier) instead of the main reasoning model.
Reserve the heavier model for STRESS-TEST/perspective review and final synthesis, where judgment is load-bearing; skip this if the host has no model-selection option.

**Fallback (no engine):**
- Seed URLs from GitHub READMEs / `awesome-*` lists / package pages, then aggregators (HN, Product Hunt, alternativeto.net, dev.to), then follow leads like engine results.
- Flag in TL;DR: "Web research limited — no search engine."
- On 401/403 → key invalid: try the other engine and tell the user to check the key in `~/.octocode/.env`. On 429/5xx → switch engine/fallback and continue. Never block on search failure.
- Never print/commit keys (`.env` is gitignored); one-off user-provided keys may be passed as env vars for verification only.

## Smart querying (all surfaces)

- **Semantic expansion** — never search only the user's words; run 2–3 synonyms/reframes in parallel (e.g. "code review" → "pull request analysis", "diff feedback", "static analysis AI"). Seed these from the Frame & Diverge slate.
- **Recency first** — GitHub: ignore repos inactive >2y (prior art, not competition). Web: default `--time-range year`; widen only if <3 results.
- **Quality filter — prefer validated sources.**
  - GitHub: skip forks/skeletons/<10★ unless sole match; prefer recent commits, engaged issues, multiple contributors.
  - **Packages (npm): downloads alone ≠ healthy** — also weigh **last-publish recency, release cadence, maintainer count, open-issue/PR ratio, and dependency freshness**.
  - A high-download but unmaintained package (last publish >1–2y, single maintainer, stale deps) is a *risk to flag*, not validation — and is often the white-space signal (popular but abandoned = opportunity).
  - For package health, ask `octocode-research` for package metadata, source repo status, commits, issues, and dependency freshness; don't trust the download badge alone.
- **Formal web ladder.** For method, technical, scientific, or safety claims, use Tavily/Serper/web search to discover, then fetch/open and cite formal sources only.
  - Cite: official/reference docs, standards bodies, protocol RFCs, arXiv, Google Scholar/Semantic Scholar leads to papers, ScienceDirect, PubMed, IEEE Xplore, CORE/open-access copies, and canonical academic/industry papers.
  - Leads only (not proof): community discussions, marketing pages, blogs, videos, listicles, HN/Reddit/StackOverflow, Product Hunt, SEO/AI-farm pages — unless the research question is explicitly about community/market sentiment.
- **Tavily/Serper filters.** Prefer `--include-domains` for formal sources (`arxiv.org,semanticscholar.org,pubmed.ncbi.nlm.nih.gov,ieeexplore.ieee.org,sciencedirect.com,core.ac.uk`, plus official docs domains) and `--exclude-domains` for noisy hosts. Google Scholar is a discovery surface; cite the fetched paper/publisher page, not the Scholar result.
- **Trend/momentum signal.** When Tavily/Serper return generic or undated pages, dispatch/use the Trend & Source Scout (above) instead of retrying the same query.
