# Unified routing held-out eval

This deterministic eval decides whether the merged `ghSearch`,
`ghSearchHistory`, and `localSearch` surface earns its place over the retired
split public tools. Correctness is evaluated before cost. A smaller catalog
cannot rescue an invalid route.

## Goal

Preserve correct research routing while reducing the public tool-selection
context carried by agents.

## KPI

- Primary (lagging): total routing bytes (`input schema +
  name/title/description`), lower is better; baseline 39,318, result 29,467,
  target at most 39,317.
- Leading metrics: execution calls, input-schema bytes, and prompt bytes.
- Correctness guardrail: both surfaces must route all 10 held-out cases.
- Cost guardrails: execution calls may not increase; schema bytes and prompt
  bytes must each decrease.
- Budget: one deterministic trial over 10 frozen cases.
- Decision: **ACCEPT** only when correctness remains 10/10 and every cost
  guardrail holds; otherwise **REVERT**.

The executable contract and baseline values are committed in
[`fixtures/unified-routing-held-out.json`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/fixtures/unified-routing-held-out.json).

## Loop level

Experiment: measure the already-merged unified catalog against one frozen
retired-surface fixture. No runtime implementation was changed in this task.

## Budget / trials

One deterministic trial over 10 held-out cases. The same cases, serializer, and
byte-counting rules apply to both surfaces.

## Surfaces

The retired fixture contains these nine public tools:

- `ghSearchCode`, `ghSearchRepos`, `ghViewRepoStructure`
- `ghSearchPullRequests`, `ghSearchIssues`, `ghSearchCommits`
- `localSearchCode`, `localFindFiles`, `localViewStructure`

Their titles and descriptions are frozen from
`@octocodeai/octocode-core@18.2.0`. Input-schema byte counts were captured from
the corresponding retained split query schemas with Zod JSON Schema
serialization. These values are fixture data only; the eval never registers or
dispatches a retired runtime alias.

The unified candidate is read live from
`DIRECT_TOOL_DISCOVERY_DEFINITIONS`. The eval therefore fails when a canonical
route stops accepting its held-out query, when a retired name returns to the
public catalog, or when the measured candidate exceeds the frozen cost gates.

## Measurement

- Correctness: the current canonical bulk input schema accepts the held-out
  query and the frozen retired tool owns the same capability.
- Calls: one execution call per objectively scoped case. Schema-help calls,
  retries, network calls, latency, and model reasoning are excluded.
- Schema bytes: UTF-8 bytes of compact `JSON.stringify(z.toJSONSchema(schema,
  { io: "input" }))` for the three unified tools versus the nine frozen split
  schemas.
- Prompt bytes: UTF-8 bytes of `name + "\n" + title + "\n" + description`.
- Total routing bytes: schema bytes plus prompt bytes. No token approximation
  is presented.

## Subject changed

None. This task adds only the held-out benchmark, fixture, and documentation;
the subject under measurement is the existing unified public catalog.

## Harness unchanged?

Yes during measurement. Correctness cases were frozen and passed before the
cost assertions and KPI thresholds were added. The final candidate measurement
uses that same case set.

## Frozen result

Observed on 2026-09-01:

| Metric | Retired split | Unified | Change |
|---|---:|---:|---:|
| Correct routes | 10/10 | 10/10 | unchanged |
| Execution calls | 10 | 10 | unchanged |
| Input-schema bytes | 33,713 | 28,291 | −16.08% |
| Prompt bytes | 5,605 | 1,176 | −79.02% |
| Total routing bytes | 39,318 | 29,467 | −25.05% |

## Verdict

**ACCEPT**. Correctness and calls are unchanged while both byte costs improve.

## Run

```bash
node node_modules/vitest/vitest.mjs run \
  packages/octocode-benchmark/tests/unifiedRoutingBenefit.test.ts \
  --coverage=false
```

## Checks run

- Held-out benchmark: 12/12 tests passed.
- Scoped ESLint: passed.
- Isolated TypeScript `--noEmit`: passed.
- Prettier check: passed.
- Documentation verification: passed.

## Transcript note

No live provider or model transcript is part of this deterministic eval. The
retired metadata and schema-byte snapshot are committed in the fixture; the
candidate catalog is loaded directly by the test.

## Scope limits

This is a held-out contract and catalog-cost eval, not an agent-trajectory or
live provider benchmark. It proves deterministic routing/schema compatibility
and public-context savings. It does not claim fewer network calls, lower
latency, or higher model routing accuracy. Add isolated agent trials separately
before making those claims.

## Next

Keep this suite as a regression gate. Run separate isolated agent trajectories
before claiming that unified routing reduces retries or improves model routing
accuracy.
