# Results — octocode-vs-ast-grep

**TL;DR 🏆 octocode — parity on clean AST, wins beyond-AST.** Counts identical where ast-grep parses (274=274, 50=50, census Δ0.11%); ast-grep is 3–4× faster per call and best-in-class at single-node extraction (Q9). octocode wins everything needing identity (LSP refs), reachability (dead exports), outlines, or **Flow-typed JS** — ast-grep `-l js` mis-parses Flow generics (confirmed by 3 independent solvers). 1.00 vs 0.75 (run b), 0.95 vs 0.90 (run c).

> Tracked results ledger. Latest scored run first; full artifacts in the (gitignored) `output/<run>/` dir it names. Refresh this file after every scored run (see BENCHMARK.md § Results ledger).

## Run: compare-run-20260802-b (first scored run of this suite)

- **Time of check:** 2026-08-02 ~16:20–16:45 IDT (solves) · judged same session
- **Verdict: AST PARITY CONFIRMED (Q1–Q5 tie 5.0/5.0, counts equal or attributed) · BEYOND-AST WIN for octocode (Q6–Q10: 5.0 vs 2.5).** Overall 1.00 vs 0.75.
- Provenance: corpus `context/react` @ `9ceb1e7d` (verified) · ast-grep **0.45.0** (`-l js`) + shell glue · octocode CLI v18.0.0 (local build:dev) · k=1, single-agent solve-then-judge (not blind) · GT answers recomputed at run time per suite rule
- Artifacts: `output/compare-run-20260802-b/` — `octocode-vs-ast-grep.md`, `kpi.json`, `logs/octocode-vs-ast-grep/*/calls.jsonl`

### Performance comparison matrix

| Metric | A: ast-grep | B: octocode | Note |
|---|---:|---:|---|
| Correctness — all 10 | 0.75 | **1.00** | |
| Correctness — AST parity zone (Q1–5) | **1.00** | **1.00** | tie — engine parity |
| Correctness — beyond-AST zone (Q6–10) | 0.50 | **1.00** | identity/reachability/outline/composite |
| Raw stdout bytes | 194,195,619 (‼) | **345,377** | A's Q5 `--json` dump alone = 193 MB |
| Raw bytes excl. Q5 | 1,058,022 | **245,531** | 0.23× |
| Tool calls | 18 | 17 | |
| Tool wall-clock | **2.6 s** | 6.1 s | ast-grep is very fast per call |
| Q5 census wall-clock (cold) | **0.43 s** | 1.88 s | 122,925 vs 122,791 calls (Δ0.11%, attributed) |

### Per-question matrix

| Q | Topic | A | B | Result |
|---|---|---:|---:|---|
| 1 | `useState(…)` count (reconciler/src) | 1.0 | 1.0 | **274 = 274** exact |
| 2 | `$OBJ.push(…)` set (react/src) | 1.0 | 1.0 | **50 = 50**, full `file:line` sets identical |
| 3 | divergence reconciliation (reconciler/src) | 1.0 | 1.0 | ast-grep **342** vs octocode **332**; +10 attributed to **Flow mis-parse artifacts** — 4–272-line spans with no `.push(` at match start (e.g. `ReactFiberScope.js:36` span 84, `ReactTestSelectors.js:232` span 30, `ReactChildFiber.js:282` span 162); all 332 B-matches verified to contain a literal `.push(`; text grep 347 = superset. **Stand behind 332.** |
| 4 | `await` inside `try` (relational rule) | 1.0 | 1.0 | **1 = 1** (`__tests__/setupTests.js:63`), genuine `inside: try_statement` rule both arms |
| 5 | whole-corpus call census | 1.0 | 1.0 | 122,925 @ 0.43 s vs 122,791 @ 1.88 s; Δ134 = 0.11% < 0.5% gate, direction matches Q3 attribution; caps lifted both |
| 6 | `scheduleUpdateOnFiber` cross-file refs | **0.5** | 1.0 | B (LSP references, identity-guaranteed): 5 external files — BeginWork 3, ClassComponent 4, **Hooks 6**, HotReloading 2, Reconciler 13. A's call-shape count missed non-call refs (Hooks 3 vs 6) and cannot exclude name collisions by construction |
| 7 | scheduler dead exports + verification | **0** | 1.0 | B: 31 candidates (confidence:low, honest) + 2 independent verifications — `frameYieldMs` **REFUTED** (live via `src/forks/Scheduler.js` build-fork wiring the entrypoint resolver can't see), `runIdCounter` **CONFIRMED** (all 4 occurrences internal to SchedulerProfiling.js). A: export enumeration collapsed on Flow (1 match), no reachability concept |
| 8 | ReactFiberWorkLoop outline | **0.5** | 1.0 | Oracle recomputed at SHA: **125** top-level fns (63 export + 62 bare; GT's stale "~99" superseded — record in GT next revision). B: 125 via 3-page symbols outline, **55,294 B vs 203,233 B = 0.272**. A: 51+24=75 via two patterns (Flow generics break the rest), no outline surface, 400 KB match dumps |
| 9 | bounded read of `scheduleUpdateOnFiber` | **1.0** | 1.0 | Both lines 973–1099. **A shines**: exact AST node extract, 4,403 B, 1 call. B: anchor + range, 5,165 B, 2 calls |
| 10 | composite find→outline→read `flushSyncWork` | **0.5** | 1.0 | Both: def `react-reconciler/src/ReactFiberWorkLoop.js:1934` (`export function flushSyncWork(): boolean`), distractor local fn `ReactFiberConfigDOM.js:5013` excluded. A needed a **grep fallback** (its def-pattern failed on the Flow return type) = "extra manual steps". B: 3 calls, 14,189 B, full accounting |

### Conclusion

Where ast-grep's grammar parses cleanly, the two engines agree **exactly** (Q1/Q2/Q4; Q5 within 0.11%) — and ast-grep is 3–4× faster per invocation with a brilliant node-extraction read (Q9, its best result). The differences are systematic: (1) **Flow-typed JS mis-parses** — ast-grep 0.45.0 `-l js` produces spurious multi-line matches on generic/annotated signatures (+10 false positives in Q3, 40% missed functions in Q8, failed def-pattern in Q10) while octocode's structural engine handles the same files cleanly; (2) **no beyond-AST surface** — identity references (Q6), reachability (Q7), and outlines (Q8) are simply not expressible, which is the suite's designed ceiling (`astGrepCeiling`). Watch item for octocode: Q3/Q5 result payloads (129 KB/100 KB) are heavy for counting questions. GT correction queued: Q8 oracle "~99" → 125 at `9ceb1e7d`.

## Run: compare-run-20260802-c (subagent re-run, blind judge)

- **Time of check:** 2026-08-02 16:36–17:01 IDT · independent solvers + blind judge
- **Verdict: B ahead 0.95 vs 0.90 (quality 4.7 vs 4.1).** Both workers independently rediscovered the Flow-typed-JS parse degradation and attributed their count divergences (A: "tree-sitter parse degradation… undercounts"; B: "AST both misses real calls and fabricates spurious nodes — cross-check required"). Engine finding now confirmed by 3 independent solvers across 2 runs → top work item: Flow-aware grammar or documented fallback.

- **Flow (trajectory judge):** A **5/5** and B **5/5** — both textbook (relational rules, caps lifted, divergences attributed not averaged). Value score (corr×qual×flow) B/A = **1.21×**; A's read-bytes unreported so REQ n/a.

## Prior runs

| Run | Date | Verdict | Corr B vs A | Notes |
|---|---|---|---|---|
| compare-run-20260802-b | 2026-08-02 | parity + beyond-AST WIN | 1.00 vs 0.75 | first scored run; promoted from "needs scored run" |
| compare-run-20260802-c (subagents, blind judge) | 2026-08-02 (eve) | B ahead | 0.95 vs 0.90 | independent workers re-confirmed the Flow mis-parse divergence from both sides |
