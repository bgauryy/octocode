# Research algorithm

Load when the task class or first move is unclear. The strongest handle decides the first move; never force a fixed grep → AST → LSP pipeline.

| Handle | First move |
|---|---|
| none | docs entry points, else tree depth 1-2 + match counts; re-enter at hotspots |
| concept/behavior | guess a literal or synonym alternation (`a\|b`) → anchors → `matchString` read; `clasify` for a described target inside a known large file or via a search handoff |
| identifier | text search or `workspaceSymbol`; LSP when identity matters |
| code shape | `astSearch operation:"match"` |
| file topology | `octocode graph` or beta `astTopology` |
| installed package | resolved version → `artifactSearch` `version` → `next.viewReleaseSource` |
| why/history | PR/commit history on the path; issue → `closedBy` → fix PR |

Pick a surface and a task; skip irrelevant or redundant stages when a known anchor already answers.
- Surface: local → `workflow-local.md`; remote, docs, or local ↔ remote → `workflow-external.md`.
- Task: lookup → direct read; bug → `workflow-debug.md`; feature/enhancement/refactor → `workflow-change.md`; review → `workflow-pr-review.md`; consequential claim → `code-research.md`; multi-pass → `campaigns.md`.

## Problem contract
`actual | expected | authority | trigger | impact | success criteria | non-goals`. Authority: test, spec, schema, documented promise, accepted user criterion, or established behavior.

| Class | Evidence test | Done when |
|---|---|---|
| bug | a supported contract is violated | reproduction or equivalent, mechanism, alternate disconfirmed, regression check |
| feature | a new contract is needed | consumers, acceptance tests, compatibility decision |
| enhancement | contract holds; a metric must improve | baseline, target, experiment, regression guard |
| unknown | actual or authority unresolved | one missing fact + cheapest check or focused question |

Model only the load-bearing path `entry → transformations → state → output → consumers`. Bugs locate the first divergent boundary; features the smallest boundary that can own the new criterion.

Proof dimensions: structure (location, layout) · stream (exact text) · connections (graph, LSP, AST). A nontrivial claim uses two; failure and empty-result handling → `octocode.md`.

Handoff only `question | scope/ref | evidence/confidence/gaps | checks | next`.

Next: load the route this table picked; a lookup answered by one exact read ends here.
