# Octocode vs `ast-grep` — Structural Search Benchmark

10 tasks against a **frozen React checkout**. Lanes: **parity** (both tools
apply — counts must match exactly on a pinned scope), **reconciliation**
(engineered divergence that must be *attributed*, not averaged), and
**beyond-AST** (semantic identity, reachability, bounded reading).

- **Arm A (`ast-grep`)**: ONLY the `ast-grep` CLI (`ast-grep run -p '<pattern>'`,
  `ast-grep scan --inline-rules`, `ast-grep outline`). Local files only.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`
  (`localSearchCode mode:"structural"`, plus its other local surfaces).

## Corpus (shared by ALL local-tool suites)

Both arms run against the same **pinned** checkout — never against the
octocode repo itself (a live repo drifts under the benchmark; a pinned corpus
doesn't — counts moved 7555→8512 in one working day when we benchmarked
against our own source).

```bash
git clone https://github.com/react/react.git packages/octocode-benchmark/context/react
git -C packages/octocode-benchmark/context/react checkout 9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d
```

- Pinned commit: `9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d` (2026-07-27).
  Verify with `git -C $CORPUS rev-parse HEAD` before any run; if it moved,
  re-seed the ground truth.
- The checkout is gitignored (`packages/octocode-benchmark/.gitignore`).
- ~1,873 Flow-typed `.js` files under `packages/` — the same scale ast-grep's
  own end-to-end benchmark uses (opencode, 2,311 TS files), and deliberately
  *dirty* input: tree-sitter-javascript cannot parse Flow annotations, so both
  engines exercise error recovery (Q3 turns that into a scored question).
- React is famous → contamination risk. Run the no-tools control arm first
  (shared method in [`../README.md`](../README.md)).

| Q | Lane | Tests |
|---|---|---|
| Q1 | parity | Call-shape count (`useState($$$)`) — identical count (274) |
| Q2 | parity | Member-call sites — identical count AND `file:line` set (50) |
| Q3 | reconcile | Same pattern where engines diverge (342 vs 332) — attribution required |
| Q4 | parity | Relational YAML rule (`inside` try) — octocode accepts ast-grep rule YAML verbatim |
| Q5 | scale | Whole-corpus census (~123k matches) + cold wall-clock KPI |
| Q6 | beyond | Cross-file callers with identity (LSP references, 28 refs / 5 files) |
| Q7 | beyond | Dead-export candidates + verification discipline (reachability) |
| Q8 | parity | Outline surfaces head-to-head (`ast-grep outline` vs `minify:"symbols"`) |
| Q9 | beyond | Bounded read: one function's bytes, not the 203 KB file |
| Q10 | beyond | Composite find→outline→read flow with distractor symbols |

## Why these

`ast-grep` is excellent at local AST matching, and 0.45 added `outline` — so
parity questions meet it at full strength (Q8 is outline-vs-outline, not a
strawman). The beyond lane tests what patterns cannot express: identity
(Q6 — name hits vs resolved references), reachability (Q7), and byte-bounded
reading (Q9/Q10). Q3 and Q5 adopt ast-grep's own benchmark lesson (their
tree-sitter-rewrite series): end-to-end numbers on a real corpus diverge from
micro-numbers, and divergences must be *attributed one boundary at a time* —
a solver that averages or cherry-picks a count scores 0.

## Oracle status

- **Seeded 2026-08-02** on the pinned SHA with ast-grep 0.45.0 and the local
  octocode build: every expected count/set/attribution in `ground-truth.json`
  was computed by running BOTH tools and diffing normalized `file:line` sets.
  Parity self-verification still applies, but on a pinned corpus the seeded
  numbers are stable oracles — a mismatch means the SHA moved, the scope
  differed (`-l js` vs `include:["*.js"]`), or a tool regressed.
- `harnessRules` in `ground-truth.json` lists every gotcha that produced a
  false divergence (0/1-based lines, file-set scope, caps, `$$X`, relative
  paths, Flow error-recovery, modifier semantics) — apply before comparing.

Shared method + metrics (three arms incl. no-tools control, trajectory
grading, aggregation, validity gates): [`../README.md`](../README.md).
