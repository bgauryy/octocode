# Workflow: Combination (Local + External)

Use when no single surface can answer: a local clue points upstream, or remote code needs AST, LSP, negative, or many-file proof. Read `references/algorithm.md` first. This bridges `references/workflow-local.md` and `references/workflow-external.md`.

## Local -> External (enrich)
- local dependency / error string / config key -> `npmSearch` or `ghSearch(operation:"repositories")` -> repo -> docs, tests, history.
- "why is this code like this" -> `ghSearchHistory(operation:"commits")` on the path -> `ghGetHistoryItem(operation:"commit", ref:<sha>)` -> search PR history, then fetch the matching PR with `operation:"pullRequest"` and `number` as needed.
- "has someone already solved this" -> `ghSearch(operation:"repositories")` triage -> external loop on the best candidates (`references/github-landscape.md` for ranking several).

## External -> Local (materialize, then prove)
One bridge call turns remote code into local-grade evidence; the full local loop then runs unmodified on the returned `localPath`.

| Depth | Call | Lands on disk | Use when |
|---|---|---|---|
| tree | `ghGetFileContent type:"directory"` | one subtree (bounded — check `skipped` counts) | analyzing one directory |
| file | `ghCloneRepo` + `sparsePath` | that file's subtree + repo-root files (`complete:false`) | repeated reads/LSP on one file |
| repo | `ghCloneRepo` (no sparsePath) | full shallow clone (`complete:true`) | repo-wide search / AST / LSP / graph analysis |

Clone works by default on CLI and MCP. `ENABLE_CLONE=false` disables it and returns a typed error. Mark that surface skipped, fall back to file reads, and follow returned `next.localSearch` / `next.viewStructure`.

**Materialize when:** AST/structural, LSP, multi-file regex, exact absence, graph `cycles`/`reachability`/`deadCode`, or the 3rd+ read into one remote area is coming. After cloning, run graph operations on `localPath`; keep their import edges candidate-grade until exact/LSP proof.

## The loop
Loop local clue → external evidence → local proof until the claim reaches the strongest available grade. Check external facts against local reality and local upstream guesses against the source that shipped them.

Next: run the local half with `references/workflow-local.md` and the remote half with `references/workflow-external.md`; once materialized, prove the claim through `references/code-research.md`; when a clone or run needs approval read the gates in `references/octocode.md`.
