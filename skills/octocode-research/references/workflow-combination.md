# Workflow: Combination (Local + External)

Use when no single surface can answer: a local clue points upstream, or remote code needs AST, LSP, negative, or many-file proof. Read `algorithm.md` first. This bridges `workflow-local.md` and `workflow-external.md`.

## Local -> External (enrich)
- local dependency / error string / config key -> `npmSearch` or `ghSearchRepos` -> repo -> docs, tests, history.
- "why is this code like this" -> `ghSearchCommits` on the path -> the PR behind the commit via `ghSearchPullRequests` (`reviewMode:"full"` for the whole story).
- "has someone already solved this" -> `ghSearchRepos` triage -> external loop on the best candidates (`github-landscape.md` for ranking several).

## External -> Local (materialize, then prove)
One bridge call turns remote code into local-grade evidence; the full local loop then runs unmodified on the returned `localPath`.

| Depth | Call | Lands on disk | Use when |
|---|---|---|---|
| tree | `ghGetFileContent type:"directory"` | one subtree (bounded — check `skipped` counts) | analyzing one directory |
| file | `ghCloneRepo` + `sparsePath` | that file's subtree + repo-root files (`complete:false`) | repeated reads/LSP on one file |
| repo | `ghCloneRepo` (no sparsePath) | full shallow clone (`complete:true`) | repo-wide grep / AST / LSP / dead-code |

CLI clone works by default; the MCP-server surface requires `ENABLE_CLONE=true` and returns a typed error when disabled. Mark that surface skipped, fall back to file reads, and follow returned `next.localSearch` / `next.viewStructure`.

**Materialize when:** AST / structural, LSP, multi-file regex, exact absence, or the 3rd+ read into one remote area is coming.

## The loop
Loop local clue → external evidence → local proof until the claim reaches the strongest available grade. Check external facts against local reality and local upstream guesses against the source that shipped them.

Validate: `node scripts/eval-research.mjs --case campaign-combination`.
