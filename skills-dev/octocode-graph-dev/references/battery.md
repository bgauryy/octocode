# Language battery

Load when measuring a graph change, reading a battery row, or adding a grammar case.

The battery ingests one pinned repository per grammar and compares the files `callers` returns for a probe function with the files ripgrep finds calling it. It also records the agent workflow on each snapshot: `stats`, `find`, `impact`, `issues`, `cycles` and `unresolved`.

## Run

```bash
node skills-dev/octocode-graph-dev/scripts/graph-bench.mjs --out <dir>/before.json
# change, rebuild ($DEV build:dev), then
node skills-dev/octocode-graph-dev/scripts/graph-bench.mjs --out <dir>/after.json
node skills-dev/octocode-graph-dev/scripts/bench-compare.mjs <dir>/before.json <dir>/after.json
```

- Clones: `packages/octocode-benchmark/local/repos`; recreate missing ones at the pinned commits in its README. A missing clone becomes a row with an ingest error, not a skipped row.
- `--only c,rust` reruns named rows; compare only rows both runs contain.
- `--bin` (or `OCTO_BIN`) points at a lane's native binary, so a worktree build can be measured without touching the main launcher.
- `--jobs 1` for timing claims: the default runs four ingests at once, and machine load moves `ingest.ms` far more than most changes. Re-time on an idle machine before reporting a speed change.
- A full run takes minutes; each row runs `graphBuild` with `refresh: true`, which rebuilds every snapshot under each clone's `.octocode/graph/`, then reads `graphQuery` pages with `responseChars: 50000`.
- `--edges` adds every call edge into the probe (`witnesses`: from, to, line, confidence) to each row.
- Per-edge agreement with LSP: [lsp-crosscheck.mjs](../scripts/lsp-crosscheck.mjs) `--peer <graph-view data.json> --crates <dir,...> --out <file>` compares every `witnesses` call edge of each crate with an octocode-graph-view LSP cross-check.

## Reading a row

| Field | Meaning |
|---|---|
| `recall.recall` / `precision` | Files both found ÷ oracle files / ÷ graph caller files |
| `recall.missed` / `extra` | Complete file lists behind those ratios; read the source of each before calling it a defect |
| `recall.missedSites` | Every oracle site in a missed file with the graph's `unresolved` row for the probe at that line (`reason`, `targetName`); `reason: null` means the graph holds no unresolved row there |
| `recall.dropped` | Oracle hits removed as the probe's own declarations, `exclude`d files, or text inside strings and comments |
| `ingest.callLinkRate` | Linked call sites ÷ all extracted sites: syntax coverage, not semantic recall |
| `ingest.unresolvedByReason` | Why sites stayed unlinked; a moved count shows which rule a change touched |
| `callers.pagesComplete` | Every callers page was followed; `false` means the ratios are partial |

The oracle ([lib/oracle.mjs](../scripts/lib/oracle.mjs)) matches text, not bindings (`\bname\s*[(<\[]`; never `rg -w` with a trailing `(`, which matches nothing), then drops:

- hits at a `find` declaration's name (`selectionByteRange`), so a declaration-only file is not a miss;
- files matching the case's `exclude` globs (vendored copies, amalgamations), on both sides of the ratio;
- hits inside string literals and comments, by a per-extension lexical scan.

What stays text-level:

- A same-named function of another library (`_ = require('lodash')` resolving to `node_modules`) or a binding built at runtime is still a "miss"; `missedSites` shows its reason.
- Prototypes and declarations the graph does not list are still hits.
- Calls through `extern "C"` includes, re-exports or other names are real callers the oracle may not see, so an "extra" can be correct.
- [CODE_GRAPH.md](../../../packages/octocode-native/docs/engine/CODE_GRAPH.md#evaluation) records the explained misses for each row; keep that list current.

## Adding a case

Add a row to `CASES` in `graph-bench.mjs`: the clone directory, the ingest `dir`, a probe function with callers in several files, the oracle extensions, and `exclude` globs for vendored or amalgamated copies. Prefer a probe with one definition. Run the new row twice to confirm it is stable, then add it to the CODE_GRAPH.md evaluation table.
