# Language battery

The battery measures a graph change on one pinned clone per grammar: the files `callers` returns for a probe function against the files a ripgrep oracle finds calling it, plus the workflow queries on each snapshot. Run the calls, oracle and numbers with `node skills-dev/octocode-graph-dev/scripts/battery.mjs --bin <pinned binary> --out <campaign dir> [--rows <ids>] [--before <bench.json>]` ([battery.mjs](../scripts/battery.mjs); `--help` lists what it writes), then judge each row by reading it ([rules](../../../packages/octocode-benchmark/CHECKS.md#rules)).

## Run

- Clones: `packages/octocode-benchmark/local/repos/<clone>`, at the commits the [local benchmark README](../../../packages/octocode-benchmark/local/README.md) pins.
- Binary: pin the binary under test with `$DEV pin` (it prints the frozen path and sha256). From the clone, call `NODE_ENV=development OCTOCODE_NATIVE_BIN=<frozen copy> node <repo>/packages/octocode-graph/bin/octocode-graph.mjs graphBuild|graphQuery '<json>'` with `path` set to the ingest dir. Without `NODE_ENV=development` the launcher ignores `OCTOCODE_NATIVE_BIN` and runs the installed binary, warning only on stderr.
- Receipts: run every row before the change and after the rebuild; keep both runs in `packages/octocode-benchmark/campaigns/v<major>-<YYYY-MM-DD>-graph-battery/` (gitignored): `bench.json` (a record per row with every call's input), `compare.json` (before beside after, a verdict per row) and `report.md`.

| Row | Clone | Ingest dir | Probe | Oracle extensions | Exclude |
|---|---|---|---|---|---|
| asm+c | asm | `.` | `jpeg_std_error` | c | |
| c | c | `src` | `createStringObject` | c | |
| cpp | cpp | `.` | `skip_whitespace` | hpp | `single_include/**` |
| csharp | csharp | `Src/Newtonsoft.Json` | `ArgumentNotNull` | cs | |
| go | go | `.` | `FromStrings` | go | |
| java | java | `guava/src` | `checkNotNull` | java | |
| javascript | javascript | `.` | `isObject` | js | `vendor/**` |
| javascript-cjs | nextjs | `scripts` | `getGitHubToken` | js | |
| javascript-lazy | huge-ts | `scripts/chat-simulation` | `startServer` | js, ts | |
| python | python | `django` | `force_str` | py | |
| rust | rust | `tokio` | `poll_proceed` | rs | |
| scala | scala | `core` | `tailRecM` | scala | |
| typescript | typescript | `packages` | `areTestsFiltered` | ts | |
| cuda | cuda | `tools/library` | `make_gemm_real_canonical_layouts` | cu, cpp, h, hpp, cuh | |
| tsx | tsx | `packages` | `mutateElement` | ts, tsx | |

The javascript-cjs and javascript-lazy probes were picked mechanically before measuring: the export with the most caller files among modules 2+ files load by relative `require`, and the export called most through function-scope or inline relative `require` across 2+ files.

Per row:

1. `graphBuild` with `refresh: true`, then `graphQuery` `stats` with `view: "details"`: build ms, `linkage.linked`, `linkage.sites`, `calls.unresolvedByReason`. Link rate is linked ÷ sites (`linkage.ratio` divides by in-repo `eligible` sites instead); both count syntax call sites, not semantic recall.
2. `find` the probe with `kind: "symbol"` and `includeProvenance: true` (for `selectionByteRange`), then `callers` of its id with `confidence: "low"` and `view: "files"`, following every page. The default `confidence: "high"` drops links that earlier receipts count.
3. Oracle: `rg -n '\bPROBE\s*[(<\[]' DIR` over the row's extensions (`rg -w` with a trailing `(` matches nothing), minus the probe's own declaration sites (`selectionByteRange`), files matching `exclude` (on both sides of the ratio) and hits inside strings and comments (a per-extension lexical scan). Recall = files both found ÷ oracle files; precision = files both found ÷ graph caller files.
4. For each missed file, `unresolved` with `target` set to it and `edges: ["calls"]` gives the graph's reason at the oracle's line, or none.
5. `impact` on the probe's id, `issues`, `cycles` and `unresolved` each answer without an error.
6. A second `graphBuild` without `refresh` on the unchanged tree reuses the snapshot (same id) and is faster.

Record rows under the field names earlier receipts use, so runs compare field by field: `ingest.{ms,callSites,linked,callLinkRate,unresolvedByReason}`, `callers.pagesComplete` (false: the ratios are partial), `recall.{recall,precision,missed,extra,missedSites,dropped}`, `workflowOk`, `incremental.{ms,reused}`.

## Judge

- Precision must hold on every row. A recall drop, a workflow query that fails where it answered before, or `incremental.reused` turning false is a regression to explain from source; accept a recall drop only as an explained removal of a name-guess link.
- Read the source behind every miss and new extra. The oracle matches text, not bindings: a same-named function of another library or a binding built at runtime still counts as a miss (`missedSites` gives the graph's reason), prototypes and declarations the graph does not list still count as hits, and callers through `extern "C"` includes, re-exports or other names are real but invisible to it, so an extra can be correct.
- Any recall or precision change, or a cache version bump, needs the owner's acceptance with before → after rows.
- Timing claims: one row at a time on an idle machine; machine load moves build time more than most changes.
- Per-edge agreement with LSP: compare `witnesses` call edges with an octocode-graph LSP cross-check (`via: lsp | both | graph`).
- Viewer paging timing: same frozen tree, binary and warm snapshots. In the run's `events.jsonl`, count one `tool` start per id; a chunk's `paging` state confirms a native snapshot and its `done` state that the viewer read the pages. Compare streamed rows as multisets.

Keep the explained misses for each row current in [CODE_GRAPH.md](../../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation).

## Add a row

Add the clone, the ingest dir, a probe with one definition and callers in several files, its oracle extensions and `exclude` globs for vendored or amalgamated copies. Run it twice to confirm it is stable, then add it to the CODE_GRAPH.md evaluation table.
