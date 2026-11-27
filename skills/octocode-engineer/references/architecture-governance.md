Load when keeping an architecture healthy over time: fitness functions in the build, architecture metrics, hotspots or change coupling.

# Architecture: governance and metrics

## Governance and evolution

| Technique | Purpose |
|---|---|
| Architecture fitness functions | Automatically verify architecture characteristics in the build |
| Dependency rules as code, module boundary enforcement, architecture tests | Reject forbidden imports and cross-module implementation access; validate layering ([mechanisms](analysis.md#model-the-wiring)) |
| Fitness thresholds and trends | Define acceptable limits; detect deterioration over time |
| Architecture debt register | Track structural liabilities with owner and cost |
| Risk-based architecture review | Spend review effort on the most consequential decisions |
| Architecture characteristics matrix | Compare quality-attribute trade-offs across options |
| ATAM (Architecture Tradeoff Analysis Method) | Find sensitivity points, trade-off points and risks against quality attribute scenarios |
| Technical radar | Track technology adoption: adopt, trial, assess, caution (the Thoughtworks ring names; "caution" was "hold" through 2025) |
| Architecture roadmap | Plan structural evolution in reversible steps |
| Architecture knowledge base | Preserve decision rationale ([decision record](../output.md#decision-record)) |

## Architecture metrics

Structural metrics (fan-in, fan-out, instability, abstractness) are in [dependencies and coupling](concepts-coupling.md#dependencies-and-coupling). Add runtime, delivery and change measures:

| Metric | What it reveals |
|---|---|
| Cyclic dependency count, strongly connected component size | Dependency loops and how entangled they are |
| Modularity, community detection | Natural subsystem boundaries to compare with declared ones |
| Change coupling | Files or modules that change together without a static dependency |
| Code churn, hotspot score | Edit frequency; churn combined with complexity ranks refactoring candidates |
| Propagation cost | Share of the system a change can reach through dependencies |
| API surface size | Exposed contract complexity |
| Dependency depth | Layering complexity |
| Centrality, betweenness | Modules that many dependency paths pass through |
| Deployment coupling | Components that must be released together |
| Change lead time, deployment frequency, failed deployment recovery time, change fail rate, deployment rework rate | Delivery health (the five DORA metrics: throughput, then instability) |
| Tail latency, cost per request | User-felt performance; infrastructure efficiency |
| Error-budget consumption | Reliability risk ([burn rate](systems-operations.md#observability-and-sre)) |
| Architectural rule violations | Boundary compliance over time |

Rank hotspots with `node <skill-folder>/scripts/hotspots.mjs --since <YYYY-MM-DD> [pathspec ...]` ([hotspots.mjs](../scripts/hotspots.mjs); read-only git, run inside the repository). `files` are sorted by `score = commits × complexity`, where complexity is indentation depth summed over non-blank lines (`--complexity lines` counts non-blank lines instead). Each row keeps its inputs and `formula` records the calculation. Compare scores or set a threshold only between runs with the same `formula`, `--since` date and `paths`. Leave generated or vendored code out with a pathspec such as `':(exclude)dist'`. Propagation cost has several formulations: write down the exact calculation and its inputs before comparing values.
