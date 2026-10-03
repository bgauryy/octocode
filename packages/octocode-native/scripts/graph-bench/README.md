# graph-bench

These scripts measure the timing and quality of `octocode graph ingest|query`.
Neither script trusts the graph to judge itself.

```bash
B=packages/octocode-native/target/release/octocode    # cargo build --release -p octocode-cli
W=$PWD/octocode-local-testing                         # workspace inside the path policy (gitignored)
python3 packages/octocode-native/scripts/graph-bench/bench.py  $B $W octocode-local-testing/repos/{javascript,tsx,rust,go} > bench.jsonl
python3 packages/octocode-native/scripts/graph-bench/seeded.py $B $W octocode-local-testing/repos
```

## `bench.py`

Prints one JSON line per repo:

- **Throughput and size:** ingest time, files per second, snapshot bytes,
  node and edge counts.
- **Determinism:** re-ingests the repo and checks that `graph.bin` has the same
  sha256.
- **Query latency:** process wall time for `stats`, `dependents`, `impact`,
  `deps`, `callers`, `find`, `cycles`, `hubs`, and `issues`, next to the bare
  `--version` startup time.
- **Edge accuracy:** decodes `graph.bin` directly and samples 100 import edges
  and 100 high-confidence call edges. Each edge counts as correct only if the
  source line it cites names the target.
- **Detector proxies:** these are conservative lower bounds. Unreachable files
  are checked for import-like textual references with `git grep`. Unused-export
  names are checked for any occurrence in other non-test files. Each cycle
  witness hop is checked against its source line.

## `seeded.py`

Measures recall and negative controls. It copies trees into the workspace,
ingests a clean baseline, then injects known defects:

- a TypeScript cycle, a dead file, and an undeclared package
- a Python eager cycle next to a lazy function-level cycle
- a Rust dev-dependency used both in production and in `mod tests`

It then diffs the two snapshots with `issues --baseline`. Every seed must show
up as a new finding. The lazy cycle and the test-scoped import must not.

## Ground-truth oracles

Both oracles ingest the target with `octocode graph ingest` and read
`graph.bin` through `graphbin.py`, the decoder shared with `bench.py`.

```bash
python3 packages/octocode-native/scripts/graph-bench/oracle_ts.py    $B $W packages/octocode
python3 packages/octocode-native/scripts/graph-bench/oracle_cargo.py $B $W packages/octocode-native
```

- **`oracle_ts.py`** compares file→file `imports` edges with the resolutions
  that `tsc --traceResolution` reports. It uses only a local
  `node_modules/.bin/tsc` found in the project or one of its ancestors. It
  scores only importers in the tsc program and targets inside the project,
  excluding `node_modules` and `.d.ts`.
- **`oracle_cargo.py`** compares crate edges with `cargo metadata` path
  dependencies between workspace members, including root `[patch.*]` entries.
  Graph crate edges are imports between files of different cargo components.
  Precision counts any declared kind (normal, build, dev). Recall counts
  normal and build dependencies only.

Each oracle prints precision, recall, and the first five disagreements in each
direction.
