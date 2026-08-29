# Research Flow

Load for Map/Validate/Investigate/Plan execution. `references/algorithm.md` owns proof; `references/octocode.md` owns syntax; `references/workflows.md` routes debug/change/review/local/external work.

Start with a Surface Plan: local, GitHub, packages, PR/history, web, and reasons for skips. Rare extensions: `references/long-research.md` for durable/contested decisions and `references/github-landscape.md` for repo ecosystems.

## Mode Flows
| Mode | Chain |
|---|---|
| Map | literal + synonyms → repos/packages → tree/search → exact finalist reads → active/partial/abandoned/white-space clusters |
| Validate | reframe/invert/decompose → local-first if relevant → external evidence → cross-pollinate → Advocate/Critic → build/narrow/prototype/do-not-build |
| Investigate | structure → symptom/symbol search → exact boundary reads → graph for file topology + LSP/AST for identity/shape → history/tests; keep two hypotheses |
| Plan | current contract/invariants → graph/LSP blast radius → boundary checks → local pattern → options/safest next step |

Package evidence includes publish recency, cadence, maintainers, issue/PR ratio, and dependency freshness. Gate public contracts, cross-package edits, shared deletes/renames, or broad consumer impact.

## Surface Recipes
```text
Docs/wiki lead: tree -> exact doc -> verify each named entry point
Local: tree/find -> search -> symbols/matchString -> graph when topology matters -> LSP/AST
Remote/package: package/repo search -> tree -> code search -> exact read -> history
Remote as local: directory fetch/clone -> local AST/LSP/search (bridge: references/workflow-combination.md)
PR intent: PR metadata/comments/selected patches -> exact changed paths -> history
Graph: dependencies/dependents/path/cycles/reachability → exact edge reads → LSP when symbol identity matters
Dead code: `localAnalyzeGraph(operation:"deadCode")` candidates → exact read + text/AST + LSP excluding declaration + tests/build
```

## Cross-Pollination
- Local dependencies/errors/config feed external queries; upstream fixes/history return to local proof.
- README competitors become repo/package checks; issue complaints become PR/commit searches.
- Empty results get one synonym/scope/ref adjustment, then materialization before strong absence.
- Compress large outputs into `claim → evidence → confidence → next` before continuing.

## Advocate / Critic
State the strongest cited case for and against; rebut the claim most likely to flip the decision; keep survivors, drop concessions, and expose unresolved decision points.

## Before Answering
- Corpus/ref and active/skipped surfaces are explicit.
- Raw tool schemas were read; continuations/pagination were followed or declared unnecessary.
- Candidates became exact evidence; syntax/semantic/history/artifact/runtime proof are distinguished.
- LSP uses a real anchor; empty/incomplete semantic results use another lane.
- Output cites local `path:line` and remote URL/PR/commit IDs, names diagnostics/fallbacks, and reports verification.

Next: after repeated Act→Observe→Learn cycles or shifting verification load `references/loop-mode.md`; when a code claim needs the proof ladder load `references/code-research.md`; when the decision must be durable load `references/long-research.md`; when the plan turns into edits go to `references/workflow-change.md`.
