Load when the task class or mode is unclear: bugs, features, enhancements, repository comparisons. Proof bar for root causes and consequential claims (callers, cycles, reachability, deletion, architecture): [proof](proof.md). Multi-pass plans, loops, workers, handoffs: [campaigns and handoff](campaigns-handoff.md). Tool choice and calls: [tool flow](tool-flow.md).

# Method

## Classify the task

Skip stages a known anchor answers; a lookup is a direct read.

| Class | Definition of Done |
|---|---|
| bug | supported contract violated, all four [root cause](proof.md#root-cause) conditions hold, regression check exists |
| feature | consumers, acceptance tests, and a compatibility decision set |
| enhancement | contract holds; baseline, target, experiment, regression guard exist |
| unknown | one missing fact has its cheapest check or a focused question |

## Pick a mode

| Mode | Use when | Chain |
|---|---|---|
| Map | area or owners unknown | literal + synonyms → repos/packages → tree/search → exact finalist reads → clusters |
| Validate | idea, claim, or build-vs-reuse | reframe/invert → local → external → Advocate/Critic → build, narrow, or drop |
| Investigate | symptom needs a cause | structure → symptom search → exact boundary reads → graph/LSP/AST → history/tests |
| Plan / Architecture | change needs its safest step | contract → entry points → graph/LSP scope → exact boundaries → safest step/tradeoffs |

- Pick surfaces (local, GitHub, packages, PR/history, docs/web) by the claim; name spanned surfaces you skip. Local ↔ upstream: [tool flow](tool-flow-carry.md#carry-evidence-across).
- Advocate/Critic: strongest evidence for and against; rebut the claim most likely to flip the decision.
- Reuse (repository landscape): discover repos and packages; separate fit from risk; verify finalists via source, tests, releases, issues, license; report comparison, integration path, gaps. Popularity proves nothing.
