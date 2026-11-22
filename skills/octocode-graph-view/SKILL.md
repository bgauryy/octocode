---
name: octocode-graph-view
description: "Load this skill first when asked for an interactive repository map or saved graph brief. Build, query, serve or replay dependency graphs and add source-backed architecture notes."
---

# Octocode Graph View

## Build the requested scope

Run from the target workspace. When environment capabilities are unknown, use `node <skill>/scripts/preflight.mjs`; disclose missing layers that affect the task. Leave repository source untouched.

| Need | Command |
|---|---|
| A person will watch | `node <skill>/scripts/serve.mjs <section>` opens the viewer; `<section>` is the folder to map, such as `.` for the whole repo or `packages/api`. Give the printed URL for live progress and on-demand AST outlines or LSP callers, callees and references. |
| Headless build or CI | `node <skill>/scripts/build.mjs <section>` writes the same graph, events and briefs. |
| Reopen a finished run | `node <skill>/scripts/serve.mjs --replay <result-dir>` |

The pipeline pages `structureSearch`, builds package chunks and stitches imports and calls; `scopeTooBroad` triggers splitting. Use `--whole` when cross-package linkage matters more than early results: splitting a Rust module tree can lose calls. See [commands](references/commands.md) for flags, phases, metrics and script owners.

## Check a finding

1. Read `brief.txt` for measured facts, then inspect the deciding rows in `data.json`. Nodes and edges carry `loc` with paths and lines. Check the file's `check.status` and `verification.staleCheck` before relying on a row. Rebuild for `stale`, `missing` or out-of-range `lines`. Disclose a failed freshness check.
2. Imports are syntax evidence. Calls carry `via`: graph linkage (`graph`), LSP alone (`lsp`) or agreement (`both`). Viewer `stitched` edges use exact names across chunks (`resolution: ["stitched-import"]`), medium confidence. LSP probes hub functions and stitched sources. Check coverage before counts or absence; confirm consequential claims with exact source, symbol identity and LSP callers. Test defect hypotheses against the [evidence rules](references/evidence.md).
3. Report the deciding `data.json` rows or source lines. Put coverage gaps beside the claims they limit, including split or unbuilt chunks, files outside chunks, unresolved imports, stale checks and skipped LSP. If blocked, give the build state.

| After the build | Read |
|---|---|
| Find paths, impact, cycles, callers or rankings | [Graph queries](references/query.md) |
| Answer a viewer question or refresh after edits | [Chat and refresh](references/chat.md) |
| Save reusable source notes | [Wiki operations](references/wiki.md). Write with `wiki.mjs note` and check with `wiki.mjs lint`. |
| Add layers, runtime edges or flows | [Model overlay](references/modeling.md) and [schema](scheme/model.json). Save beside `data.json` and rerun `render.mjs`. |
| Deliver the viewer, brief or raw evidence | [Output](output.md). Raw pages are under `chunks/<id>/tmp/evidence/` and events are in `events.jsonl`. |
