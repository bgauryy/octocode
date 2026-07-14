# Web Search Worker Topology

Load when brainstorm research needs 2+ engines or multiple query angles. Dispatch per `octocode-subagent` (`GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → SYNTHESIZE`).
One bounded objective per worker; a self-contained packet (query, engine, framing, return shape — workers inherit no parent context); a synthesis barrier before the parent reconciles results. Stay within the five-worker ceiling.

- **Web Search Scout (×1 per validated engine):** one engine, one query slice; return ranked fetched leads with title/url/date/author, run in parallel across engines.
- **Aggregator:** merge the Scouts' results after the barrier — canonicalize and dedupe by URL, apply the strong/medium/weak confidence tiers from `references/tools.md`, surface conflicts, drop SEO/farm noise.
  Fold into the parent when only 2-3 Scouts ran; spawn separately only if the merge itself is large enough to earn its own worker per `spawn-gate.md`.
- **Source/Code Checker:** validate the aggregated leads through formal sources and `octocode-research`.
- **Trend & Source Scout:** when momentum/crowdedness needs `trend-sources.md` evidence.

Run all validated Web Search Scouts + Source/Code Checker as the default closed loop (typically 3-4 workers); add Trend only for a distinct question.
**Deep dive when needed:** if the consolidated evidence stays thin, single-engine-only, or materially conflicting after the first round, don't stop.
Reframe the query (2-3 synonyms), dispatch a second parallel Scout round with the new framing, or hand the gap to Source/Code Checker for a formal-source pass before synthesizing.
Use a fast worker tier for mechanical fetch/summarize when supported. Reserve judgment for stress-test/synthesis — treat every worker's output as a claim to re-check, per `octocode-subagent`.

Surface selection and confidence tiers: `references/tools.md`.
