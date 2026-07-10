# Workflow: Combination (Local + External)

Use when no single surface can answer: a local clue points upstream, or remote code must be proven with AST / LSP / negative / many-file evidence that only local tools give. Read `algorithm.md` first; this is the bridge between `workflow-local.md` and `workflow-external.md`.

## Local -> External (enrich)
- local dependency / error string / config key -> `npmSearch` or `ghSearchRepos` -> repo -> docs, tests, history.
- "why is this code like this" -> `ghHistoryResearch` commits on the path -> the PR behind the commit (`reviewMode:"full"` for the whole story).
- "has someone already solved this" -> `ghSearchRepos` triage -> external loop on the best candidates (`github-landscape.md` for ranking several).

## External -> Local (materialize, then prove)
One bridge call turns remote code into local-grade evidence; the full local loop then runs unmodified on the returned `localPath`.

| Depth | Call | Lands on disk | Use when |
|---|---|---|---|
| tree | `ghGetFileContent type:"directory"` | one subtree (bounded — check `skipped` counts) | analyzing one directory |
| file | `ghCloneRepo` + `sparsePath` | that file's subtree + repo-root files (`complete:false`) | repeated reads/LSP on one file |
| repo | `ghCloneRepo` (no sparsePath) | full shallow clone (`complete:true`) | repo-wide grep / AST / LSP / dead-code |

Clone works out-of-the-box in the CLI (`npx octocode`); the `ENABLE_CLONE=true` gate applies only to the MCP-server surface, where an unset gate returns a typed error naming the requirement. When gated off, treat it as a skipped surface, not a dead end — fall back to `ghGetFileContent` reads. Use the returned `next.localSearch` / `next.viewStructure`.

**Materialize when:** AST / structural, LSP, multi-file regex, exact absence, or the 3rd+ read into one remote area is coming.

## Federated in one shot (OQL)
`oqlSearch from:{kind:"github",owner,repo}` plans provider search plus optional materialization (`materialize:"auto"/"required"`). GitHub rows come back as provider-grade text with a prefilled `next.fetch` to upgrade to exact content; zero rows plus `providerUnindexed` is a blind spot, not absence — follow `next.materialize`.

## The loop
Local clue -> external evidence -> local proof, until the claim rests on the strongest available grade. Cross-pollinate at least once each way: check every external fact against local reality where possible, and confirm every local guess about upstream against the repo, PR, or package that actually shipped it.

Validate: `node scripts/eval-research.mjs --case campaign-combination`.
