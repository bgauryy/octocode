# Octocode Graph against other code-structure tools, 2026-11-23

A capability round (one checking agent, direct calls, no blind judge): Octocode Graph against native
graphQuery, lspSearch, rg, ast-grep, madge, dependency-cruiser and Graphify on a large Rust workspace and a small
TypeScript package. Every number below was measured in this round unless it is labelled an estimate. Raw inputs
are in [data/](data/).

## BLUF

- **Depth: Octocode Graph is the only tool here that answers cross-package structure in one call.** Transitive
  impact across 11 crates (697 files for `result.rs`, 0.36 s), paths, cycles, rankings, communities, layering
  order, staleness warnings and offline replay all come from one saved graph. Native graphQuery answers the
  same kinds of question inside one crate or package; madge and dependency-cruiser cover TypeScript file
  imports only; LSP, rg and ast-grep need a manual walk.
- **Callers on Rust are where Graph is worse.** On 20 Rust functions (143 call sites), Graph found 57% of the
  sites with no false positives. lspSearch found 97%, and an `rg` call-pattern search found 100%. Graph missed
  44 of the 72 calls from other crates and 13 of the 22 calls inside macros, and its answer did not say it was
  incomplete (`isPartial: false`, no warning). Every cross-crate call it did find was backed by LSP evidence.
- **On the small TypeScript package every tool is accurate.** Graph got 100% on callers, imports, impact,
  paths and the injected cycles, at 0.1 s per question. Its extra cost is the build: 5.2 s the first time,
  1.4 s without LSP, and 127 s on the Rust workspace (60% of that in LSP).

## Headline: callers

Precision and recall are per call site. Tests count for every tool (Graph is run with `--tests`). Times are
the median wall time of a fresh process per call. Characters are the median length of the default text
output.

| Tool | Rust P | Rust R | Rust caller-fn R | TS P | TS R | Rust first / warm (s) | TS warm (s) | Rust chars | Calls per question |
|---|---:|---:|---:|---:|---:|---|---:|---:|---:|
| **Octocode Graph** `callers <name> --tests` | 1.00 | **0.57** | 0.56 | 1.00 | 1.00 | 0.36 / 0.36 | 0.10 | 440 | 1 |
| native graphQuery (one crate/package) | 1.00 | 0.27 | 0.31 | 1.00 | 0.76 | 0.43 / 0.44 | 0.16 | 2,267 | 2 (`find` + `callers`) |
| lspSearch `callers` (rust-analyzer / tsserver) | 1.00 | 0.97 | 0.97 | 1.00 | 1.00 | 0.59 / 0.09 | 0.07 | 523 | 1 (needs file and line) |
| lspSearch `references` | 0.90 | 0.97 | 0.97 | 0.76 | 1.00 | 0.14 / 0.09 | 0.08 | 693 | 1 |
| `rg -n -w NAME` | 0.87 | 1.00 | 1.00* | 0.72 | 1.00 | 0.11 / 0.06 | 0.01 | 486 | 1 |
| `rg` call pattern `\bNAME\s*(::<…>)?\(` | 1.00 | 1.00 | 1.00* | 1.00 | 1.00 | 0.06 / 0.06 | 0.01 | 374 | 1 |
| `ast-grep run -p 'NAME($$$)'` | 1.00 | 0.23 | 0.22 | 1.00 | 1.00 | 0.51 / 0.48 | 0.02 | 88 | 1 |
| ast-grep, 3 patterns (bare, `$P::`, `$R.`) | 1.00 | 0.85 | 0.88 | 1.00 | 1.00 | 0.78 / 0.73 | 0.03 | 445 | 1 |
| Graphify `explain NAME --all` | 1.00 | 0.24 | 0.28 | 1.00 | 0.71 | 2.14 / 2.14 | 0.15 | 794 | 1 |

\* rg and ast-grep do not name the calling function; their caller-function recall is computed from the
sites they list. Their perfect precision is a property of the sample: every function has a unique name in its
target (see [Method](#method)). On shared names, a spelling search cannot tell the functions apart.

Sample sizes: Rust has 20 functions and 143 sites (105 production, 38 test). TypeScript has 13 functions and
63 sites. First-call medians leave out 3 Rust functions that a pilot had already called (n = 17). Warm
medians use calls 2 and 3.

### Where Graph's Rust misses are (143 sites)

| Site category | Sites | Graph | native | LSP callers | rg call | ast-grep 3 patterns | Graphify |
|---|---:|---:|---:|---:|---:|---:|---:|
| Same crate, outside macros | 49 | 45 | 32 | 46 | 49 | 49 | 26 |
| Other crate, outside macros | 72 | **28** | 0 | 72 | 72 | 72 | 6 |
| Inside a macro (`json!`, `assert_eq!`, `tokio::join!`, `vec!`) | 22 | **9** | 6 | 21 | 22 | 1 | 2 |
| Production only | 105 | 59 | 26 | 102 | 105 | 98 | 25 |

- **Cross-crate calls come from LSP.** Of the 82 sites Graph found, the cross-crate ones were 22 LSP-only and
  6 confirmed by both LSP and syntax, with none from syntax alone. Name binding through stitched imports did
  not follow re-exports: none of the 8 calls to `read_private_config` from the cli crate were found, because
  the cli imports it through `config.rs`, which re-exports it with `pub use edit::{…}`. LSP probing covered
  1,683 functions, those "in hub files and sources of calls bound between chunks", as the graph's `brief.txt`
  states.
- **Calls inside macros are invisible to syntax-based tools.** This holds for Graph, native graphQuery,
  ast-grep and Graphify alike. Graph finds a call inside a macro only when LSP confirms it.
- **Method calls on receivers whose type needs inference** (4 same-crate test sites, such as `client` taken
  from a destructured tuple inside an async closure) were missed by Graph and native graphQuery.
- **Graph found calls the other tools missed.** These were calls through an alias in TypeScript
  (`const group = topGroup; group(e.source)`, 2 sites, also found by native graphQuery at 1 site). They are
  scored as neutral.
- **LSP misses (4 sites):** all 4 are calls to `ToolId::reads_source`, a method on a type that a build script
  generates. rust-analyzer runs with build scripts off and returned an empty answer. lspSearch disclosed this
  (`buildScriptsOff`, `documentAnalysisErrors`) and offered a `localSearch` fallback.

## Depth: which tool can answer

| Question | Octocode Graph | native graphQuery | lspSearch | rg | ast-grep | madge (TS) | dependency-cruiser (TS) | Graphify |
|---|---|---|---|---|---|---|---|---|
| Callers | yes (R 0.57 Rust, 1.0 TS) | one crate or package | yes (R 0.97 / 1.0) | spelling only | syntax only | no | no | one site per edge |
| Callees | `callees` | `callees` | `callees` | no | manual | no | no | `explain` |
| Calls across crates or packages | yes, through LSP evidence (28/72) | no | yes (72/72) | spelling | syntax | n/a | n/a | 6/72 |
| Transitive impact | `impact`, 1 call | one crate; default depth 3 disclosed | symbol level (`callers` depth N) | manual walk | no | manual walk (`--depends`) | `--reaches`, 1 call | `affected`, default depth 2 |
| Path between files | `path` (weighted or `--hops`) | `path` | no | manual | no | no | no | `path` |
| Cycles | `cycle`, `metrics.cycles` | `cycles` | no | no | no | `--circular` | rule `no-circular` | custom projection |
| Rankings and hotspots | `top` (pagerank, betweenness, inWeight, core, depth) | `hubs` (import fan-in) | no | `uniq -c` of specifiers | no | no | `--metrics` (Ca, Ce, I) | `god-nodes` |
| Communities | `community` (modularity 0.77 on Rust) | no | no | no | no | no | no | clustering (not run: `--no-cluster`) |
| Layering | `order`, package instability in the brief | no | no | no | no | no | forbidden rules, metrics | no |
| Snapshot diff or drift | `octocode-graph diff` (not run) | `drift` with a baseline (not run) | n/a | n/a | n/a | no | `--affected` (from git) | no |
| Staleness detection | yes: query warns "9 graph files changed" | `stale` lists changed files | live | live | live | recomputes each call | recomputes each call | earlier round: none |
| Offline replay | saved run, viewer replay | saved snapshot | no | no | no | static JSON or image | JSON and `depcruise-fmt` | `graph.json`, HTML |
| `path:line` evidence on each row | yes (every call line) | yes (one line per caller) | yes | yes | yes | no | no | yes (one line per edge) |

## Speed

| Work | Rust (3,516 files) | TypeScript (70 files) |
|---|---|---|
| Graph build, LSP smart | 127.3 s (n=1; native graph 48.6, LSP 76.2, rest 2.5) | 5.18 / 3.09 / 1.53 s (n=3; LSP and native caches warm up) |
| Graph build, `--lsp off` | not run | 1.45 / 1.45 / 1.39 s |
| Graphify `update --no-cluster` | 23.0 / 21.4 s cold; 1.19 s incremental | 2.94 / 1.65 s |
| madge, dependency-cruiser | n/a | 1.2–1.5 s on every call (no saved graph) |
| Graph query, one process | 0.36 s (loading dominates: 20 callers queries in one `--stdin` process 0.31–0.34 s) | 0.10 s |
| Graph `impact` on 3 files | 0.35–0.36 s, 15–85k characters | 0.09–0.12 s, 0.9–1.5k characters |

Blast radius on TypeScript: the ground truth is the transitive importer closure of the files' import
statements.

| File (ground truth) | Graph `impact` | native `impact` | depcruise `--reaches` | Graphify `affected` | madge `--depends` walk | rg walk |
|---|---|---|---|---|---|---|
| `runtime/lib.ts` (19) | 19, 0.12 s | 19, 0.08 s | 19, 1.15 s | 18 (depth 2); 19 with `--depth 20` | 19, 20 calls, 27.6 s | 19, 20 calls, 0.20 s |
| `runtime/query-index.ts` (13) | 13, 0.09 s | 13, 0.06 s | 13, 1.16 s | 11; 13 | 13, 14 calls, 17.5 s | 13, 14 calls, 0.18 s |
| `graph-model.ts` (23) | 23, 0.10 s | 20 at depth 3 (disclosed) | 23, 1.15 s | 11; 23 | 23, 24 calls, 32.4 s | 23, 24 calls, 0.25 s |

The rg walk needs each import specifier resolved to a file at every step. Here a script did it; an agent
would do it by hand.

Rust impact, measured on the 3 files with no ground truth for the full closure:

- **Graph `impact`** reached 697, 743 and 148 files across crates.
- **Native `impact`** stays inside one crate. It reached 143, 93 and 154 files, with 2–4 pages through
  `next` cursors. It covered every same-crate file Graph listed, and it adds test files and module parents.
- **Manual LSP walk for `retraction.rs`:** the first level took one `references` batch over 3 exported items
  (5.76 s the first time, 0.10 s repeated, 2,277 characters) and agreed with Graph (`go.rs`). Reaching
  Graph's 148 files would take at least 148 such steps. That figure is an estimate; the walk was not run.

## Imports and cycles (TypeScript)

Ground truth comes from the TypeScript scanner (`ts.preProcessFile`) over all 70 files: 190 import
statements, 167 file pairs, 73 of them with a type-only import.

| Tool | Pairs | Correct | Recall | Time |
|---|---:|---:|---:|---|
| Octocode Graph (`data.json` imports) | 167 | 167 | 1.00 | build |
| madge `--json` | 167 | 167 | 1.00 | 1.2–1.4 s |
| dependency-cruiser `--ts-pre-compilation-deps` | 167 | 167 | 1.00 | 1.4–1.5 s |
| dependency-cruiser default | 114 | 114 | 0.68 (drops type-only imports) | 0.7–0.8 s |
| `rg` specifiers plus path resolution | 167 | 167 | 1.00 | under 0.01 s, plus resolution |

- **Cycles in the unmodified package:** there are none, and every tool agrees. On a copy with two injected
  cycles (a value cycle `section.ts ↔ pipeline.ts` and a type-only cycle `graph-model.ts ↔ algorithms.ts`),
  Graph, native graphQuery, madge and dependency-cruiser with type-only imports found both. Default
  dependency-cruiser found only the value cycle.
- **dependency-cruiser without TypeScript installed** cruised 0 modules and reported no error. The working
  call was `npx -p typescript -p dependency-cruiser`.
- **Rust cycles: the definitions differ, and this round has no ground truth.** In the tool-lsp crate, Graph
  reports one strongly connected set of 102 files (95 counting imports only). Its rings run through glob
  `use super::*` imports. Native graphQuery reports 7 sets, the largest with 6 files.

## Method

- **Build:** `octocode 20.2.0+c24548a1`, native binary SHA-256 `d4607d4d…3d4b1`. The tree was dirty, on
  commit `0e754e833`. Graph queries ran the source entry
  (`packages/octocode-graph/src/runtime/commands/query.ts`). Tool versions: rg 15.2.0, ast-grep 0.45.0,
  madge 8.0.0, dependency-cruiser 16.10.4 with typescript 5.9.3, graphifyy 0.9.108 (Python 3.12, code-only),
  rust-analyzer 1.99.1, Node 26.4.0.
- **Targets:** the Rust target is `packages/octocode-native`, using a saved graph built at 10:33. The
  TypeScript target is `packages/octocode-graph/src`. That package changed during the run (69 → 70 files), so
  builds were timed on the live path and every quality measurement used a frozen copy of 70 files, which
  stayed unchanged to the end.
- **Selecting functions:** a token scan of the target kept names with exactly one `fn` or `function`
  declaration and no field, variable or other declaration of the same name. From those, a seeded random
  sample (seed 20261123) was drawn per stratum. Rust: 7 in hub files, 7 called from other crates, 6
  same-crate helpers. TypeScript: 6 cross-file, 4 same-file, plus the 3 most-called names. The list is in
  `data/functions.csv`.
- **Ground truth:** every `rg -w` hit was classified as call, import or use-list, comment, declaration or
  function-value reference, and every row was read. A site is a (path, line) pair. Calls inside TypeScript
  template interpolations count as calls. Declaration lines are ignored for every tool. Function values
  passed as arguments (3) and alias calls (2) are neutral: neither a hit nor a miss. A tool line that is the
  receiver line of a multi-line method call counts as that call. One Rust file edited by another session
  after the build (`pr_finish.rs`, 1 site) is excluded for every tool.
- **Timing:** wall clock, one fresh process per call, on a shared machine with a load average of 8–13 from
  other agents. Octocode CLI calls go to an `octocode serve` that was already running, and rust-analyzer and
  the TypeScript server were already up. Treat the timings as indicative.
- **Adjudications:** all 497 misses and false positives are in `data/adjudications.csv`, each with a reason.
  - False positives of `rg -w` and lspSearch `references` are import or use lines, use lists, re-exports and
    doc comments.
  - Graph misses: 44 cross-crate sites, 13 inside macros, 4 same-crate method calls on inferred receivers.
  - native graphQuery misses: everything outside the crate, plus the second and later sites of a caller
    (one `edgeLine` per caller).
  - Graphify misses: one location per caller edge, and a nested arrow function with no node.
- **Stored data:** `data/` holds the per-question metrics, summary, ground truth, adjudications, recall by
  category, import and cycle comparison, blast radius, build times and the function list.

## Limits

- **Rust ground truth covers calls only.** Rust transitive impact and cycles have no ground truth here; those
  rows report what each tool answers, not accuracy.
- **Unique names flatter rg and ast-grep.** The sample is restricted to unique names, which favors them. On
  overloaded names, their precision would fall and LSP or Graph identity would matter.
- **Cold starts were not measured:** a cold rust-analyzer start, because the shared server was owned by
  another session, and a cold Graph build of the Rust workspace, because its 127 s comes from a single run.
  The first lspSearch calls on unopened areas took 7.1 s and 5.8 s.
- **Not exercised:** `octocode-graph diff`, native `drift`, Graphify clustering, `god-nodes` and its staleness
  behavior. The TypeScript cycle test used injected cycles, because the package has none.
- **One transient error:** one native `stale` call returned "Path does not exist" for an existing crate path.
  The same call succeeded on three retries.
