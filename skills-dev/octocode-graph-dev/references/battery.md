# Language battery

Load when measuring a graph change, reading a battery row, or adding a grammar case.

The battery checks the graph on one pinned repository per grammar: the files `callers` returns for a probe function against the files a text oracle (ripgrep) finds calling it, plus the agent workflow on each snapshot. An agent runs it by hand through the built CLI and judges each row by reading it ([method](../../../packages/octocode-benchmark/CHECKS.md)); keep no runner script.

## Run

Clones live in `packages/octocode-benchmark/local/repos` at the commits the [local benchmark README](../../../packages/octocode-benchmark/local/README.md) pins. Point `OCTOCODE_NATIVE_BIN` at the binary under test and name its hash. Run every row once before the change (`before`) and once after the rebuild (`after`); save both receipts under `packages/octocode-benchmark/campaigns/<version>-<date>-graph-battery/`.

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

The javascript-cjs probe is the exported name with the most caller files among modules two or more files load by relative `require`; the javascript-lazy probe is the export called most through function-scope or inline relative `require` across 2+ files. Both were picked mechanically before measuring (2026-11-07).

Per row:

- [ ] `graphBuild` the ingest dir with `refresh: true`; then `graphQuery` `stats` with `view: "details"`: note the build time, the call-site count, the link rate (linked ÷ sites) and `unresolvedByReason`.
- [ ] `find` the probe (`kind: "symbol"`), then `callers` of its id, following every page. The graph's caller files are the set the ratios use.
- [ ] Oracle: `rg -n '\bPROBE\s*[(<\[]' DIR` over the row's extensions, minus the probe's own declaration lines, the `exclude` globs and hits inside strings and comments. Recall = files both found ÷ oracle files; precision = files both found ÷ graph caller files.
- [ ] For each missed file, `unresolved` with `target` set to that file and `edges: ["calls"]`: the reason the graph gives at the oracle's line, or none.
- [ ] Workflow: `impact` on the probe's id, `issues`, `cycles` and `unresolved` each answer without an error.
- [ ] A second `graphBuild` without `refresh` on the unchanged tree reuses the snapshot (same id) and is faster.

Judge before against after: precision must hold on every row; a recall drop, a stage that answered before and now fails, or reuse turning off is a regression to explain from source. For a timing claim, run one row at a time on an idle machine; machine load moves build time more than most changes. For per-edge agreement with LSP, compare `graphQuery` `witnesses` call edges of a crate with an octocode-graph LSP cross-check (`via: lsp | both | graph`).

## Reading a row

| Field | Meaning |
|---|---|
| `recall.recall` / `precision` | Files both found ÷ oracle files / ÷ graph caller files |
| `recall.missed` / `extra` | Complete file lists behind those ratios; read the source of each before calling it a defect |
| `recall.missedSites` | Every oracle site in a missed file with the graph's `unresolved` row for the probe at that line (`reason`, `targetName`); `reason: null` means the graph holds no unresolved row there |
| `recall.dropped` | Oracle hits removed as the probe's own declarations, `exclude`d files, or text inside strings and comments |
| `ingest.callLinkRate` | Linked call sites ÷ all extracted sites: syntax coverage, not semantic recall |
| `ingest.unresolvedByReason` | Why sites stayed unlinked; a moved count shows which rule a change touched |
| `incremental` | A second `graphBuild` without `refresh` on the unchanged tree: `ms`, and `reused` (same snapshot id). `reused` going true → false is a regression |
| `callers.pagesComplete` | Every callers page was followed; `false` means the ratios are partial |

The oracle matches text, not bindings (`\bname\s*[(<\[]`; never `rg -w` with a trailing `(`, which matches nothing), then drops:

- hits at a `find` declaration's name (`selectionByteRange`), so a declaration-only file is not a miss;
- files matching the case's `exclude` globs (vendored copies, amalgamations), on both sides of the ratio;
- hits inside string literals and comments, by a per-extension lexical scan.

What stays text-level:

- A same-named function of another library (`_ = require('lodash')` resolving to `node_modules`) or a binding built at runtime is still a "miss"; `missedSites` shows its reason.
- Prototypes and declarations the graph does not list are still hits.
- Calls through `extern "C"` includes, re-exports or other names are real callers the oracle may not see, so an "extra" can be correct.
- [CODE_GRAPH.md](../../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation) records the explained misses for each row; keep that list current.

## Adding a case

Add a row to the table above: the clone directory, the ingest dir, a probe function with callers in several files, the oracle extensions, and `exclude` globs for vendored or amalgamated copies. Prefer a probe with one definition. Run the new row twice to confirm it is stable, then add it to the CODE_GRAPH.md evaluation table.

For graph-view timing, count one start per tool ID in `events.jsonl`. A chunk’s paging event confirms a native snapshot. Its done event confirms the viewer consumed the graph pages. For subsection builds, verify each saved file location against disk before comparing timings or LSP coverage.

For viewer paging comparisons, use the same frozen tree, binary and warm snapshots. Record wall time to the stitched checkpoint and to verification separately. Compare streamed rows as multisets (including repeated occurrences) and every edge’s provenance and lines. Count cursor calls separately from first requests.
