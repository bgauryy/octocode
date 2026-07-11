# Research Surfaces

Load when building the Surface Plan or choosing local, GitHub/package, and web evidence. `octocode-research` owns code/tool syntax; this file owns brainstorm surface choice.

## Surface Order

For external validation, start with official docs, papers, standards, canonical articles, and dated announcements. Extract leads, verify them in code/packages, then reconcile contradictions with formal sources.
Start locally for repo-targeted ideas. Skip external work for explicit local-only tasks or unavailable web.

## Local, GitHub, Packages

Delegate repo/package/history/semantic checks to `octocode-research`. Ask it to orient locally before external research when the idea touches this workspace; skip local for purely external landscapes. Carry the real stack and constraints into external queries.

## Web Engines

| Script | Credential | Best use |
|---|---|---|
| `scripts/serper-search.mjs` | `SERPER_API_KEY` | broad Google results |
| `scripts/tavily-search.mjs` | `TAVILY_API_KEY` | curated/deeper research |
| `scripts/exa-search.mjs` | `EXA_API_KEY` | AI-native/neural search, category filters (papers, GitHub, news), highlights |

Run `--check` once per engine at session start (`--presence-only` is offline-only) and record which are actually live.
A configured key is not the same as a validated one. Credentials load through `@octocodeai/config` from process env, workspace `.octocode/.env`, then global Octocode home.
Never cite snippets or print/commit keys.

**Default policy: query every validated engine, not a first-success ladder.**
Serper, Tavily, and Exa surface different result sets for the same query (raw Google SERP vs. AI-curated summary vs. neural/category-filtered) — role-based fusion, not interchangeable fallbacks.
Only fall back to fewer engines (down to DuckDuckGo, no key needed) when a key is missing or fails `--check`. Fetch/open the best formal URLs from the consolidated set → exact-read code → reconcile.

**Consolidation isn't a raw URL-overlap count.** Canonicalize URLs first (strip tracking params/fragments before comparing) — otherwise identical pages with different query strings under-merge.
Then tier confidence instead of treating "2+ engines saw it" as proof on its own.
Cross-engine SEO/aggregator pages can duplicate without independent verification, and AI-curated engines can legitimately omit a URL a raw SERP returns, so low overlap ≠ weak claim:
- **Strong:** same canonical URL from 2+ engines, each with an acceptable per-engine relevance score, ideally a primary-source domain.
- **Medium:** single engine, high relevance score.
- **Weak — flag for verification:** single engine with a low score, or a secondary/aggregator summary only.
Do not sum or compare raw scores across engines (Serper rank, Tavily score, Exa score are not on the same scale) — rank within each engine, then apply the tiers above across engines.

Worker dispatch for multi-engine/multi-angle research: `references/web-search-workers.md`.

## Query And Evidence Rules

- Expand the user's phrase into 2-3 synonyms/reframes; retry one changed shape after empty results.
- Prefer recent sources; inactive repos are prior art, not current competition.
- Package health = publish recency, cadence, maintainers, issue/PR ratio, and dependency freshness—not downloads alone.
- Formal claims prefer official docs/specs, standards, papers, and primary code/data. Community/marketing content is a lead unless sentiment is the question.
- Use domain filters for formal sources; fetch the paper/publisher page rather than citing Scholar results.
- On 401/403 switch engine and report invalid auth; on 429/5xx switch/fallback and continue. Without an engine, follow README/package/awesome-list leads and mark web coverage limited.
- Fetch 2-3 decisive sources per question; stop when another source is unlikely to change the verdict.
