---
name: octocode-graph-view
description: "Use when the user wants a visual, interactive map of a repository to explore, such as an architecture or dependency graph, or a saved source-backed graph brief. Use octocode-graph for direct dependency, caller or cycle queries."
---

# Octocode Graph View

## Build the requested scope

Run from the target workspace. When environment capabilities are unknown, use `node <skill>/scripts/preflight.mjs`; disclose missing layers that affect the task. Leave repository source untouched. `rustContext.buildScripts` executes project code; enable it only when the user's authorization covers building that project.

| Need | Command |
|---|---|
| A person will watch | `node <skill>/scripts/serve.mjs <section>` opens the viewer. Give the printed URL for live progress and on-demand AST outlines or LSP callers, callees and references. |
| Headless build or CI | `node <skill>/scripts/build.mjs <section>` writes the same graph, events and briefs. |
| Reopen a finished run | `node <skill>/scripts/serve.mjs --replay <result-dir>` |

The pipeline pages `structureSearch`, builds package chunks and stitches imports and calls; `scopeTooBroad` triggers splitting. Use `--whole` when cross-package linkage matters more than early results: splitting a Rust module tree can lose calls. See [commands](references/commands.md) for flags, phases, metrics and script owners.

## Check a finding

1. Read `brief.txt` for measured facts, then inspect the deciding rows in `data.json`. Nodes and edges carry `loc` with paths and lines. Check the file's `check.status` and `verification.staleCheck` before relying on a row. Rebuild for `stale`, `missing` or out-of-range `lines`. Disclose a failed freshness check.
2. Imports are syntax evidence. Calls carry `via`: graph linkage (`graph`), LSP alone (`lsp`) or agreement (`both`). Viewer `stitched` edges use exact names across chunks (`resolution: ["stitched-import"]`), medium confidence. LSP probes hub functions and stitched sources. Check coverage before counts or absence; confirm consequential claims with exact source, symbol identity and LSP callers. Test defect hypotheses against the [evidence rules](references/evidence.md).
3. When reusable wiki notes are requested, write source-backed notes for relevant pages with `wiki.mjs note` and check them with `wiki.mjs lint`; see [wiki operations](references/wiki.md). If the task needs layers, runtime edges or flows, add a [model overlay](references/modeling.md) matching the [schema](scheme/model.json) alongside `data.json`, then run `render.mjs` again.
4. Deliver according to [output.md](output.md). The viewer pages rows with “Show more”; every raw graph page is under `chunks/<id>/tmp/evidence/` and events are in `events.jsonl`. Report unbuilt or split chunks, files outside chunks, unresolved cross-chunk imports, skipped LSP and partial results beside the claims they limit. Done when the question is answered from cited `data.json` rows or source lines, or the build state and blocker are reported.

Use [graph queries](references/query.md) for impacts, paths, cycles, callers and rankings, or [chat and refresh](references/chat.md) to answer through the viewer or update it after edits. Use `octocode-research` to settle code or symbol claims and `octocode-engineer` for layering or refactor decisions.
