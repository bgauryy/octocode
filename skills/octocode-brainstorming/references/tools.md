# Research Surfaces

Load when building the Surface Plan or choosing local, GitHub/package, and web evidence. `octocode-research` owns code/tool syntax; this file owns brainstorm surface choice.

## Surface Order

For external validation, start with official docs, papers, standards, canonical articles, and dated announcements. Extract leads, verify them in code/packages, then reconcile contradictions with formal sources.
Start locally for repository-targeted ideas. Skip external work for explicit local-only tasks or unavailable web.

## Local, GitHub, Packages

Delegate repository/package/history/semantic checks to `octocode-research`. Ask it to orient locally before external research when the idea touches this workspace; skip local for purely external landscapes. Carry the real stack and constraints into external queries.

## Web Engines

Run `--check` only for engines you can use (`--presence-only` is offline-only) and record which are live.
A configured key is not the same as a validated one. Credentials load through the vendored `scripts/octocode-config.mjs` from process env, workspace `.octocode/.env`, then global Octocode home.
Never cite snippets or print/commit keys.

Choose the smallest useful engine set: Serper for breadth, Tavily for curated research, and Exa for neural/category search. Add a second engine when coverage, independence, or conflict resolution matters; use DuckDuckGo when no keyed engine is available. Fetch formal URLs, exact-read relevant code, and reconcile the evidence.

**Consolidation isn't a raw URL-overlap count.** Canonicalize URLs first (strip tracking params/fragments before comparing) — otherwise identical pages with different query strings under-merge.
When results come from multiple engines, tier confidence instead of treating overlap as proof.
Cross-engine SEO/aggregator pages can duplicate without independent verification, and AI-curated engines can legitimately omit a URL a raw SERP returns, so low overlap ≠ weak claim:
- **Strong:** same canonical URL from 2+ engines, each with an acceptable per-engine relevance score, ideally a primary-source domain.
- **Moderate:** single engine, high relevance score.
- **Weak — flag for verification:** single engine with a low score, or a secondary/aggregator summary only.
Do not sum or compare raw scores across engines (Serper rank, Tavily score, Exa score are not on the same scale) — rank within each engine, then apply the tiers above across engines.


## Query and evidence rules

- Expand your phrase into 2-3 synonyms/reframes; retry one changed shape after empty results.
- Prefer recent sources; inactive repos are prior art, not current competition.
- Package health = publish recency, cadence, maintainers, issue/PR ratio, and dependency freshness—not downloads alone.
- Formal claims prefer official docs/specs, standards, papers, and primary code/data. Community/marketing content is a lead unless sentiment is the question.
- Use domain filters for formal sources; fetch the paper/publisher page rather than citing Scholar results.
- On 401/403 switch engine and report invalid auth; on 429/5xx switch/fallback and continue. Without an engine, follow README/package/awesome-list leads and mark web coverage limited.
- Fetch the few decisive sources needed; stop when another source is unlikely to change the verdict.

## Worker topology

When independent engines or query angles earn delegation, dispatch per `octocode-subagent` with one bounded objective and a self-contained packet (query, engine, framing, evidence standard, return shape) per worker. Start with the smallest topology:
- **Web Search Scout** (one per validated engine): one query slice; return ranked fetched leads with title/url/date/author.
- **Aggregator** (fold into the parent for 2-3 Scouts): after the barrier, canonicalize and dedupe URLs, apply the tiers above, surface conflicts, drop SEO noise.
- **Source/Code Checker** for load-bearing claims via `octocode-research`; **Trend Scout** only for a distinct momentum question (`references/trend-sources.md`).
Treat every worker output as a claim to re-check. If evidence stays thin, reframe once, then hand the precise gap to a checker.
