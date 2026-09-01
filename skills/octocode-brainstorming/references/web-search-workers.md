# Web Search Worker Topology

Load when brainstorm research needs independent engines or query angles and delegation earns its cost. Dispatch per `octocode-subagent`.
Give each worker one bounded objective and a self-contained packet: query, engine, framing, evidence standard, and return shape. Reconcile only after all required workers return or are marked partial.

- **Web Search Scout (×1 per validated engine):** one engine, one query slice; return ranked fetched leads with title/url/date/author, run in parallel across engines.
- **Aggregator:** merge the Scouts' results after the barrier — canonicalize and dedupe by URL, apply the strong/medium/weak confidence tiers from `references/tools.md`, surface conflicts, drop SEO/farm noise.
  Fold into the parent when only 2-3 Scouts ran; spawn separately only if the merge itself is large enough to earn its own worker per `spawn-gate.md`.
- **Source/Code Checker:** validate the aggregated leads through formal sources and `octocode-research`.
- **Trend & Source Scout:** when momentum/crowdedness needs `trend-sources.md` evidence.

Start with the smallest topology that can answer the question. Add Source/Code Checker for load-bearing claims and Trend only for a distinct momentum question.
If evidence stays thin or conflicting, reframe once or hand the precise gap to a checker before deciding whether more research can change the verdict.
Use a fast worker tier for mechanical fetch/summarize when supported. Reserve judgment for stress-test/synthesis — treat every worker's output as a claim to re-check, per `octocode-subagent`.

Surface selection and confidence tiers: `references/tools.md`.
