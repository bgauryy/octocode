# Native Graph Domain — Scope

Status: **SCOPE / design** (not yet implemented).
Tracks ARCHITECTURE.md §"Research Graph Direction": move reachability/dead-code
from tool-specific JS into a generic native graph domain.

> **⚠ BLOCKED — premise invalidated.** This plan's verification strategy
> (§4: differential-test the native port against the JS `analyzeResearchFlow`
> pipeline as a "golden oracle") depends on `octocode-tools-core/src/oql/research/analyze/`,
> which has been deleted along with the rest of the OQL subsystem. There is no
> more JS reachability implementation to port from or verify against. Before
> resuming this work: either capture a golden-verdict snapshot from history
> (last commit before the OQL removal) to use as a frozen oracle, or design a
> new verification strategy (e.g. hand-labeled corpus) that doesn't require a
> live JS reference implementation. The native `graphFacts` API this plan
> builds on is also currently orphaned — see ARCHITECTURE.md's "Research Graph
> Direction" section.

## 0. Why this is not a greenfield build (the key finding)

Research into the current code (2026-07) changed the framing. Reachability and
dead-code detection **already exist and ship today** — but entirely in
TypeScript, inside the OQL analyze pipeline:

- **Per-file syntax facts ARE native** — `extract_graph_facts`
  (`src/signatures/graph_facts.rs`, JS/TS via OXC `js_oxc.rs`, ~24 langs via
  tree-sitter) emits `{declarations, imports, exports, calls, edges(contains|calls)}`
  per file, 0-based UTF-16 ranges, `symbol:{file}#{name}` ids.
- **Cross-file linking + graph algorithms are JS** — `octocode-tools-core`
  `src/oql/research/analyze/`:
  - `source-graph.ts` — `buildFileGraph()` (resolve relative import specifiers →
    files), `collectEntrypoints()` (manifest + fallbacks), `reachableFiles()`
    (iterative DFS) → `unused-file` verdicts.
  - `symbol-scoring.ts` — `scoreSymbols()` with `findAstRetainingFiles()`
    (retainers from native `imports`/`calls`), `tokenAppears()` lexical fallback
    → `reachable` / `transitive-dead` / `candidate-unused-export` /
    `unused-export`, tagged `retentionSource: 'ast' | 'ripgrep'`.
- **SCC exists nowhere** (JS or Rust). Cyclic-but-dead clusters (mutually
  referencing unused files) are the known blind spot of the current DFS.

So the native graph domain is a **strangler port + one new algorithm**, not a
from-scratch design — and the existing JS implementation is a ready-made
**differential-test oracle** (see §4). That is what makes this tractable.

## 1. Goal → KPI contract (per `octocode-graph-eval`)

**Goal.** Move file+symbol linking and the graph algorithms (reachability,
retainer lookup, SCC, transitive-dead pruning) into a native Rust graph domain
that is faster, deterministic, and language-neutral — without changing the
answers agents get, except where SCC strictly improves them.

| | Definition |
|---|---|
| **Primary KPI** | **Parity**: on a frozen corpus, native verdicts (`unused-file`, `unused-export`, `transitive-dead`, `reachable`) match the current JS pipeline's verdicts, EXCEPT SCC-only gains, which must be separately justified against hand-labeled cases. |
| **Leading KPI** | **Latency**: whole-repo analyze wall-clock, native vs JS, on the same corpus. Faster is a *hypothesis to measure* (fewer NAPI round-trips, no JS graph build), NOT an assumed win — the baseline in §4 settles it. |
| **Guardrail 1 (untunable)** | **No new false-positive deletions.** Native must not mark as dead anything the JS pipeline (or an LSP cross-check) proves reachable. A regression here is a REJECT regardless of latency. |
| **Guardrail 2** | **Fallback preserved.** When facts are missing (unsupported lang, parse error), native must degrade to the same lexical/`ripgrep` retention the JS path uses — never silently drop to "dead". |
| **Guardrail 3** | **Proof discipline unchanged.** Output stays *candidate*-grade; deletion still requires the `target:graph proof:"lsp"` upgrade. Native graph is discovery+structure, NOT deletion-grade proof (ARCHITECTURE §62-65). |
| **Decision rule** | ACCEPT only when verdict parity meets a threshold **pre-registered from the oracle baseline** (§4) — fixed once the corpus is captured and its verdict distribution is known, deliberately NOT a number invented here — **and** guardrails hold **and** latency does not regress. SCC additions ship behind their own hand-labeled acceptance. |

## 2. Design — native modules (new `src/graph/`)

Rust owns language-neutral facts + deterministic algorithms; nothing
tool-specific. The domain must provide the capabilities below; the exact module
split is an implementation detail decided **while building** (likely files under
a new `src/graph/`, but not locked here — let the code shape it):

- **Cross-file linking** — consume the per-file `GraphFacts` already produced by
  `signatures/graph_facts.rs`; resolve import specifiers to files (port the
  relative + `index.*` resolution from `source-graph.ts`, blind spots and all —
  see §7); build the file-graph (nodes=files, edges=imports) and symbol-graph
  (nodes=`symbol:file#name`, edges=`contains`/`calls`/`imports`).
- **Reachability** — iterative traversal from a caller-supplied entrypoint set
  (entrypoint *policy* stays in tools-core; native takes the resolved set).
  Ports `reachableFiles()` + symbol reachability.
- **Retainer lookup** — which reachable files/symbols reference a target (ports
  `findAstRetainingFiles` + `calleeRefersToSymbol`), preserving the
  lexical-fallback flag (`retentionSource: ast | ripgrep`).
- **SCC** (the one genuinely new algorithm — exists nowhere today) — a
  strongly-connected-components pass (e.g. Tarjan) over the file/symbol graphs so
  a mutually-referencing cluster with no external retainer is correctly
  `transitive-dead` instead of falsely `reachable`.
- **Transitive-dead pruning** — fixpoint removal of nodes whose only retainers
  are themselves dead (condense SCCs first, then prune the resulting DAG).
- **NAPI edge** — a thin binding exposing at minimum a whole-graph build; whether
  it also exposes incremental queries (`reachable`, `retainers`, `sccs`,
  `deadNodes`) is a shape decision to make against real call patterns, not up
  front. NAPI types stay at the edge (ARCHITECTURE rule).

Fact extraction stays in `signatures/` (already there); the new graph code
consumes its output — no duplication of parsing.

## 3. Boundary (unchanged from ARCHITECTURE)

- **Native graph** = language-neutral facts + deterministic structure/algorithms.
  Discovery + structure grade. NOT deletion proof.
- **LSP** = the semantic-proof layer (identity, references, callers/callees, call
  hierarchy). Deletion-grade claims still require `proof:"lsp"`.
- **Text/ripgrep** = discovery + the retention fallback only.
- **tools-core / OQL** = entrypoint policy (framework/package entrypoints,
  manifest parsing), packet shaping, and the agent-facing `target:graph` view.
  These do NOT move into Rust.

## 4. Verification — the JS pipeline is the oracle (differential TDD)

The current JS analyze pipeline is a working reference. Exploit it:

1. **Freeze a corpus** of repos already clonable by the benchmark harness
   (this monorepo + a handful of the benchmark's `repo/` fixtures), pinned by sha.
2. **Golden capture**: run today's JS `analyzeResearchFlow` over the corpus and
   snapshot every verdict (`{file/symbol id → verdict, retentionSource}`).
   This is the oracle — captured BEFORE any Rust exists.
3. **Differential test** (TDD, red→green): the native `buildGraph` result, fed
   through the same OQL packet builder, must reproduce the golden verdicts.
   Any diff is either a bug (fix Rust) or an SCC-only improvement (must be
   hand-labeled and moved to the SCC acceptance set — never silently accepted).
4. **Strangler rollout**: put the native graph behind the SAME OQL
   `target:graph` interface, behind a flag (`OCTOCODE_NATIVE_GRAPH`). Ship
   dark → parity-gate → flip default → delete the JS graph algorithms only
   after N clean runs. The JS impl stays as the fallback until then.
5. **Property tests** (Rust): reachability is monotonic in entrypoints; a node
   reachable in the file graph is never pruned; SCC condensation is acyclic;
   `dead ∪ reachable == all` and `dead ∩ reachable == ∅`.

## 5. Anti-gaming / validity

- Golden verdicts are captured from the JS oracle ONCE and frozen; do not
  regenerate them from the Rust under test (that grades the port against itself).
- The FP guardrail (§1) is the hard gate: a wrongly-dead symbol is worse than a
  missed-dead one, because agents act on deletion suggestions.
- SCC gains require hand-labeled truth, not "the new code says so."
- Report corpus coverage + any files where facts were unavailable (fallback
  fired) — silent fallback reads as "analyzed" when it was heuristic.

## 6. Milestones

1. **Oracle capture** — freeze corpus + snapshot JS verdicts + latency baseline. *(No Rust yet; establishes the KPI.)*
2. **`graph/link.rs` + `reachability.rs`** — file-level parity with `source-graph.ts` on the corpus.
3. **`graph/retainer.rs`** — symbol-level parity with `symbol-scoring.ts` (incl. `retentionSource` fallback).
4. **`graph/scc.rs` + `prune.rs`** — cyclic-dead detection; hand-label the new-verdict deltas.
5. **NAPI + OQL strangler flag** — native behind `target:graph`, differential-gated, JS fallback retained.
6. **Flip default** after clean parity runs; delete JS graph algorithms; keep lexical fallback.

## 7. Risks / limitations (stated, not scored away)

- **Import resolution is the hard part**, not the graph math. The JS resolver is
  relative-path + index-file heuristics; it does NOT do tsconfig `paths`,
  workspace aliases, or bundler resolution. Porting it faithfully means porting
  its blind spots too — do not "improve" resolution silently or parity breaks.
  Alias/monorepo resolution is a separate, later scope.
- **Cross-language graphs**: facts exist for ~24 langs but import-linking is only
  meaningful where specifiers resolve to files (JS/TS strongest). Native must
  emit facts-only (no linking) for languages whose module systems aren't modeled,
  same as today.
- **Deletion proof stays LSP.** This domain raises *discovery* quality (SCC,
  speed); it does not replace the `proof:"lsp"` gate. Don't let a faster graph
  tempt anyone into treating structure as proof (§12 anti-pattern in the manifest).
- Scale beyond a large monorepo is untested, same caveat as the rest of the loop.

## 8. First milestone (smallest thing that moves the KPI from "undefined")

Capture the JS oracle: a script that runs `analyzeResearchFlow` over the frozen
corpus and writes `graph-oracle/<repo>@<sha>.json` verdict snapshots + a latency
number. Until that exists, "native graph is at parity" is unmeasurable — and
parity is the whole acceptance gate.
