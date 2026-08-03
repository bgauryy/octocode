# Octocode vs `ast-grep` — Structural Search Benchmark (v2)

Suite version: **2**. The completed version 1 suite and its evidence were removed from the tree on
2026-08-03 and are not comparable with v2.

- Question bank ID: `ast-grep-react-v2` — questions and oracle live ONLY in
  [`../../questions/local-code/ast-grep-react-v2/`](../../questions/local-code/ast-grep-react-v2/);
  this folder defines how to run the comparison

Ten tasks run against a **frozen React checkout**. Lanes: **parity** (both tools
apply on the same pinned scope), **reconciliation** (independent sealed outputs
are compared and attributed by a judge), and
**beyond-AST** (semantic identity, reachability, bounded reading).

- **Arm A (`ast-grep`)**: ONLY the `ast-grep` CLI (`ast-grep run -p '<pattern>'`,
  `ast-grep scan --inline-rules`, `ast-grep outline`). Local files only.
- **Arm B (`octocode`)**: ONLY `node packages/octocode/out/octocode.js`
  (`localSearchCode mode:"structural"`, plus its other local surfaces).

Each solver uses only its assigned arm and seals its complete Q1–Q10 output
before seeing the other arm's output. Solvers do not invoke the other arm,
generic search/read utilities, or unspecified cross-checks. After both outputs
are sealed, an independent judge normalizes anchors, compares the outputs, and
reconciles any discrepancy from the reported evidence and inspected spans.

## Corpus (shared by ALL local-tool suites)

Both arms run against the same **pinned** checkout — never against the
octocode repo itself (a live repo drifts under the benchmark; a pinned corpus
doesn't — counts moved 7555→8512 in one working day when we benchmarked
against our own source).

```bash
git clone https://github.com/facebook/react.git packages/octocode-benchmark/context/react
git -C packages/octocode-benchmark/context/react checkout 9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d
```

- Pinned commit: `9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d` (2026-07-27).
  Verify with `git -C $CORPUS rev-parse HEAD` before any run; if it moved,
  re-seed the ground truth.
- The checkout is gitignored (`packages/octocode-benchmark/.gitignore`).
- ~1,873 Flow-typed `.js` files under `packages/` — the same scale ast-grep's
  own end-to-end benchmark uses (opencode, 2,311 TS files), and deliberately
  *dirty* input. Parser behavior on Flow syntax is measured by the run and
  reconciled from evidence; it is not assumed in the solver prompt.
- React is famous → contamination risk. Run the no-tools control arm first
  (shared method in [`../README.md`](../../README.md)).

| Q | Lane | Tests |
|---|---|---|
| Q1 | parity | Call-shape count on an identical pinned scope |
| Q2 | parity | Member-call count and complete normalized `file:line` set |
| Q3 | reconcile | Independent same-pattern outputs, suspicious-span inspection, judge attribution |
| Q4 | parity | Relational search for `await` inside `try` |
| Q5 | scale | Whole-corpus census + cold wall-clock KPI |
| Q6 | beyond | Cross-file references with identity evidence |
| Q7 | beyond | Dead-export candidates + verification discipline (reachability) |
| Q8 | parity | Complete top-level function outlines + measured read-byte cost |
| Q9 | beyond | Bounded read of one function |
| Q10 | beyond | Composite find→outline→read flow with distractor symbols |

## Why these

`ast-grep` 0.45 added `outline`, so Q8 compares complete outline results and
their measured cost rather than imposing a fixed byte envelope. The beyond
lane tests identity (Q6), reachability (Q7), and byte-bounded reading (Q9/Q10).
Q3 and Q5 require evidence for discrepancies rather than averaging or
cherry-picking counts.

## Oracle status

- Version 2 retains the pinned SHA and judge-only v1 evidence. Q8's stale
  function-count oracle was corrected from the retained
  v1 results (removed; see git history).
- Version 2 is **not yet scored**. Every scored run must verify the pinned SHA,
  run both arms independently, seal both outputs, and use an independent judge.
- `harnessRules` in `ground-truth.json` lists every gotcha that produced a
  false divergence (0/1-based lines, file-set scope, caps, `$$X`, relative
  paths, parser recovery, modifier semantics) — the judge applies these only
  after the solvers seal their outputs.

Shared contracts: [method and metrics](../../README.md) ·
[run instructions](../../INSTRUCTIONS.md) ·
[report template](../../REPORT_TEMPLATE.md) ·
[tracked results](../../results/octocode-vs-ast-grep.md).
