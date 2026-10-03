# Research Surfaces

Load when building the Surface Plan or choosing local, GitHub/package, and web evidence.

## Surface Order

Extract leads, verify them in code/packages, then reconcile contradictions with formal sources. Skip external work for explicit local-only tasks or unavailable web.

## Local, GitHub, Packages

Skip local for purely external landscapes. Carry the real stack and constraints into external queries.

## Web Engines

`--presence-only` is offline-only; record which engines are live.
A configured key is not the same as a validated one. Credentials load through the vendored `scripts/octocode-config.mjs` from process env, workspace `.octocode/.env`, then global Octocode home.
Never print or commit keys. Use DuckDuckGo when no keyed engine is available.

Canonicalizing strips tracking params and fragments. Tier cross-engine confidence instead of treating overlap as proof: SEO/aggregator pages duplicate without independent verification, and AI-curated engines can omit a URL a raw SERP returns.
- **Strong:** same canonical URL from 2+ engines, each with an acceptable per-engine relevance score, ideally a primary-source domain.
- **Moderate:** single engine, high relevance score.
- **Weak — flag for verification:** single engine with a low score, or a secondary/aggregator summary only.
Serper rank, Tavily score, and Exa score use different scales.

## Query and evidence rules

- Prefer recent sources; inactive repos are prior art, not current competition.
- Package health = publish recency, cadence, maintainers, issue/PR ratio, and dependency freshness—not downloads alone.
- Use domain filters for formal sources; fetch the paper/publisher page rather than citing Scholar results.
- Without an engine, follow README/package/awesome-list leads and mark web coverage limited.

## Worker topology

When independent engines or query angles earn delegation, dispatch per `octocode-subagent` with one bounded objective and a self-contained packet (query, engine, framing, evidence standard, return shape) per worker. Start with the smallest topology:
- **Web Search Scout** (one per validated engine): one query slice; return ranked fetched leads with title/url/date/author.
- **Aggregator** (fold into the parent for 2-3 Scouts): after the barrier, canonicalize and dedupe URLs, apply the tiers above, surface conflicts, drop SEO noise.
- **Source/Code Checker** for load-bearing claims via `octocode-research`; **Trend Scout** only for a distinct momentum question (`references/trend-sources.md`).
Treat every worker output as a claim to re-check. If evidence stays thin, reframe once, then hand the precise gap to a checker.
