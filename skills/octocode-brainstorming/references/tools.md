# Tools — research surfaces

Load when starting research (step 4) or whenever you need the exact command/flag for a surface. Covers top resources/web engines, GitHub/packages/code (Octocode CLI), the local workspace, and cross-surface query craft.

## Default external loop — resources first

For external validation, prior-art mapping, method claims, or market/technical landscapes, start with top resources before repository/package/code search:

1. Search/read top resources: official docs, papers, standards, canonical industry articles, credible practitioner writeups, and dated announcements.
2. Extract project, repo, package, author, technique, and failure-term leads from those resources.
3. Search GitHub/packages/code from those leads, then exact-read READMEs, source files, issues, PRs, and package metadata.
4. Loop back to web/resources to reconcile contradictions, stale repos, unsupported claims, and missing context.

Only skip this order when the user explicitly asks for local-only/no-web work, web access is unavailable, or the idea is purely about this workspace. For repo-targeted ideas, do a quick local orientation first or in parallel so resource queries use the real stack.

## GitHub & packages — Octocode CLI

Run `npx octocode <command> ... --no-color`. Prefer quick commands; use raw tools only for schema-exact fields or bulk — and read `tools <name> --scheme` before any raw `--queries` call.

| Tool | Use for |
|------|---------|
| `search --target packages` / raw `npmSearch` | npm libraries and source repos |
| `search --target repositories` / raw `ghSearchRepos` | Repos by topic, language, stars |
| `search owner/repo --tree` / raw `ghViewRepoStructure` | How a similar project is organized |
| `search <kw> owner/repo` / raw `ghSearchCode` | Confirm a concept is actually implemented |
| `search owner/repo/path --content-view exact` / raw `ghGetFileContent` | Read key files for specific answers |
| `search --target pullRequests` / `search --target commits` / raw `ghHistoryResearch` | How similar features shipped in PRs/commits |

Code follow-up flow after resource leads: `search --target repositories` (resolve named projects/terms) → `search --target packages` (resolve packages) → `search --tree`/text search (orient) → `search --content-view exact` (exact evidence) → `search --target commits` / `search --target pullRequests` (change history).

## Local workspace — orient here first when the idea touches the user's own repo

The `search` command auto-routes local paths: use `search <path> --tree`, `search <query> <path>`, `search <path> --search path|content|both`, and `search <file> --content-view exact|compact|symbols`; add `search <file> --op definition|references|callers|hover` for semantics. Raw tools: `localSearchCode`, `localFindFiles`, `localGetFileContent`, `localViewStructure`, `lspGetSemantics`.

| Tool | Use for |
|------|---------|
| `search <path> --tree` / raw `localViewStructure` | How the workspace is laid out; symbol outline of a file |
| `search <kw> <path>` / raw `localSearchCode` | Is this concept *already* implemented here? (`--pattern`/`--rule` for AST shape) |
| `search <path> --search path` / raw `localFindFiles` | Locate files by name/path/content |
| `search <path> --content-view symbols` / raw `localGetFileContent` | Read the exact code — signatures or full |
| `search <file> --op …` / raw `lspGetSemantics` | Call sites, callers, references → blast radius of a change |

**When to use:** the idea is grounded in *this* repo — "should we add X to **our** app", "is Y worth building into **our** codebase", "does **our** system already do Z". Then **orient locally before external research**, so you (a) don't recommend reinventing something the repo already has, and (b) frame every prior-art query with the workspace's real stack, libraries, and naming. **Skip entirely** for purely external ideas (market size, landscape, "has anyone built X out there") — local adds nothing there.

Local orient flow: `search <workspace> --tree` (structure) → `search <concept> <workspace>` / `search <workspace> --search path` (does it exist already?) → `search <file> --content-view symbols` (how it works) → `search <file> --op ...` (who depends on it / blast radius). Carry the real lib names, framework, and constraints you find into the GitHub/npm/web queries — local findings sharpen external search the same way cross-pollination does.

## Web/top resources — search → read → follow

Two interchangeable engines in `scripts/` (same CLI: `--query --max-results --time-range --check --presence-only --help`; same JSON `{engine,answer,results[{title,url,content}]}`):

| Script | Key | Best for |
|--------|-----|----------|
| `serper-search.mjs` | `SERPER_API_KEY` | Fast Google SERP, broad coverage |
| `tavily-search.mjs` | `TAVILY_API_KEY` | AI-curated answers, deep research |

- **Check once at startup:** `node <skill_dir>/scripts/<engine>-search.mjs --check`. Use whichever exits 0; if both, Serper for breadth + Tavily for depth. `--check` performs a live authorization check; add `--presence-only` only when offline. Both exit 1 → tell user once: add `SERPER_API_KEY` (serper.dev) and/or `TAVILY_API_KEY` (app.tavily.com) to `<absolute skill_dir>/.env`.
- **Loop:** run engine → fetch/open the best formal URLs with the runtime web reader (`WebFetch` in Claude; web/open tool or Browser in Codex) → extract repo/package/code leads → read exact code → return to resources for contradictions and context. Search snippets are leads; cite only fetched/opened sources.
- Engine flags: Tavily `--depth basic|advanced|fast|ultra-fast`, `--topic general|news|finance`, `--include-domains`/`--exclude-domains` (comma-separated), `--start-date`/`--end-date` (YYYY-MM-DD), `--auto-parameters`, `--max-results` (0–20); Serper `--gl`, `--hl`, `--time-range`.
- **Worker brief** (per web slice): research <slice> → run engine → read the best **formal/validated** URLs → report who's doing it, what's right/wrong, gaps, best URLs with author/date notes; cite all.

**Fallback (no engine):** seed URLs from GitHub READMEs / `awesome-*` lists / package pages, then aggregators (HN, Product Hunt, alternativeto.net, dev.to), then follow leads like engine results. Flag in TL;DR: "Web research limited — no search engine." On 401/403 → key invalid, try the other engine, give the absolute `.env` path; on 429/5xx → switch engine/fallback and continue. Never block on search failure. Never print/commit keys (`.env` is gitignored); one-off user-provided keys may be passed as env vars for verification only.

## Smart querying (all surfaces)

- **Semantic expansion** — never search only the user's words; run 2–3 synonyms/reframes in parallel (e.g. "code review" → "pull request analysis", "diff feedback", "static analysis AI"). Seed these from the Frame & Diverge slate.
- **Recency first** — GitHub: ignore repos inactive >2y (prior art, not competition). Web: default `--time-range year`; widen only if <3 results.
- **Quality filter — prefer validated sources.** GitHub: skip forks/skeletons/<10★ unless sole match; prefer recent commits, engaged issues, multiple contributors. **Packages (npm): downloads alone ≠ healthy** — also weigh **last-publish recency, release cadence, maintainer count, open-issue/PR ratio, and dependency freshness**. A high-download but unmaintained (last publish >1–2y, single maintainer, stale deps) package is a *risk to flag*, not validation — and is often the white-space signal (popular but abandoned = opportunity). Read `search --target packages` output and the source repo's `search --target commits`/issues, don't trust the download badge alone.
- **Formal web ladder.** For method, technical, scientific, or safety claims, use Tavily/Serper/web search to discover, then fetch/open and cite formal sources only: official docs/reference docs, standards bodies, protocol RFCs, arXiv papers, Google Scholar/Semantic Scholar leads to papers, ScienceDirect, PubMed, IEEE Xplore, CORE/open-access copies, and canonical academic/industry papers. Community discussions, marketing pages, blogs, videos, listicles, HN/Reddit/StackOverflow, Product Hunt, and SEO/AI-farm pages are leads only unless the research question is explicitly about community/market sentiment.
- **Tavily/Serper filters.** Prefer `--include-domains` for formal sources (`arxiv.org,semanticscholar.org,pubmed.ncbi.nlm.nih.gov,ieeexplore.ieee.org,sciencedirect.com,core.ac.uk`, plus official docs domains) and `--exclude-domains` for noisy hosts. Google Scholar is a discovery surface; cite the fetched paper/publisher page, not the Scholar result.
