# Code graph quality plan

This plan tracks the quality work on `octocode graph ingest|query`. Every item
has an acceptance test that is written first and fails before the fix lands.
Every item is re-measured with `scripts/graph-bench/` on real repositories.

## Baseline (2026-09-28)

| Measure | Value | Source |
|---|---|---|
| Import-edge accuracy (sampled) | 89–100% | `bench.py` edge proxy |
| High-confidence call-edge accuracy | 98–100% | `bench.py` edge proxy |
| Call internal recall | Rust 0.30 · Java 0.24 · Go 0.42 · Python 0.47 · TSX 0.79 | `calls.unresolvedByReason` / `callInternalRecall` |
| Unlinked calls: unknown-receiver method | 7–32% of call sites | same |
| Unlinked calls: `Type.method` unmatched | 0–12% of call sites | same |
| Unlinked calls: ambiguous name | 2–15% of call sites | same |
| Dead-code precision (lower bound) | 0.15–0.6 on large repos | `bench.py` detector proxies |
| Dominant dead-code false positive | files referenced by strings (workers, build/extension configs), ~90% on vscode | sample review |
| `impact` from one file (excalidraw) | 537 files | seeded run |
| Linux snapshot | 800 MB, 1.2 s open | `bench.py` |

Call internal recall is linked calls divided by (linked calls plus unlinked
calls whose name exists in the repo).

## Items

| # | Item | Why | Acceptance (test first) |
|---|---|---|---|
| 1 | **Coverage in answers.** `callers`, `callees`, `impact`, and `path` over calls report `coverage` (the language's call internal recall and a warning when it is below 0.9). | Agents treat an incomplete caller list as complete. | A query on a fixture with an unlinkable member call reports `coverage.callInternalRecall < 1` and a `warning`. |
| 2 | **Dead-code confidence tiers and name-mention check.** Before reporting an unreachable file or unused export, one repo scan looks for the stem or name in strings, configs, JSON/YAML/HTML, and scripts. A match lowers the finding to tier 60 and cites `mentionedIn`. Tiers are 100/90/60. `--min-confidence` defaults to 90. Exports used only inside their own file become `export-only-local`. | String references are the dominant false positive. The research leaders (SCARF, vulture, knip) all work this way. | A file referenced only by `new Worker('./w.ts')` or a JSON config is not reported at the default tier. A truly orphaned file is reported at tier ≥90. An export used only in its own file becomes `export-only-local`. |
| 3 | **`Type.method` resolution and scoped disambiguation.** Resolve the type through an import binding, the same file, the same directory or package, then a unique type name, and take its member. For ambiguous names, prefer the caller's file, then its package or directory, then imported files. | Recovers up to 12% plus 15% of call sites (Java). | `Foo.bar()` links to `Foo.bar` in another file of the same package. A name defined in two packages links to the same-package definition. |
| 4 | **Receiver-type facts in the parser.** Call facts carry `receiverType` from declared types, parameter annotations, constructor assignments, and `self`/`this` field types. The linker resolves `x.m()` to `Type.m` with `via: receiver-type`. | Unknown-receiver calls are the largest recoverable bucket at 7–32%. | Rust `let s = Store::new(); s.save()`, TS `const s: Store = …; s.save()`, Python `s = Store(); s.save()`, and Java `Store s = new Store(); s.save()` all link to `Store.save`. |
| 5 | **Actionable `impact`.** Default `--depth 3`, barrels pass through (importers of a pure re-export file follow the symbols they bind), and output is grouped as `willBreak` / `likely` / `shouldTest`. | 537 files is not actionable. | Changing a file behind a barrel does not reach barrel importers that bind other names. The default depth is 3 and it is reported. |
| 6 | **`astTopology` end-to-end.** The shared resolver changed, so the public tool is checked through the real CLI and MCP paths plus contract fixtures. | A public tool changed as a side effect. | CLI and MCP `astTopology` on fixtures and on this repo return contract-valid output. Contract and CLI suites pass. |
| 7 | **Snapshot format v2.** Sorted, front-coded strings; symbol keys stored as suffixes; varint records; adjacency rebuilt at load. | 800 MB and 1.2 s on Linux. | Round-trip and determinism tests pass. v1 is rejected with a re-ingest message. Linux snapshot is at most 40% of v1. |
| 8 | **Ground-truth oracles.** The bench compares import edges with `tsc --traceResolution` (TS) and crate edges with `cargo metadata` (Rust). | The proxies are text-based and must not become the only yardstick. | The oracle scripts report precision and recall. Import precision is ≥95% where an oracle runs. |
| 9 | **Unchanged-repo reuse.** `graph ingest` reuses the latest snapshot of the same scope when every file digest and the file set are unchanged (`--force` rebuilds). | Agent loops re-ingest constantly. | A second ingest of an unchanged tree returns `reused: true` without parsing. An edit forces a rebuild. |
| 10 | **Discoverability.** `octocode-research` points at the graph for structure and blast-radius questions. | Only CLI users who read `--help` find it today. | The skill review passes. The skill text names `graph ingest` and `graph query impact`. |

## Gates for every change

- `cargo clippy -p octocode-native -p octocode-cli -p octocode-engine --all-targets` is clean.
- `tools::ast_graph` tests pass, including the new acceptance tests.
- `seeded.py` recalls every seed and leaks no negative control.
- `bench.py` on the 20-repo set:
  - Call precision stays ≥98%.
  - Call internal recall rises.
  - Dead-code precision lower bounds reach ≥0.8 at the default tier.
  - Snapshots stay deterministic.
