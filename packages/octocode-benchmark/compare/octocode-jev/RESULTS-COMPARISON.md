# Results — WITH vs WITHOUT Jev (A/B, Q1–Q30, 2026-09-19)

Both arms: same 30 shared questions, same Sonnet runner design (one continuous
session, local CLI surface), same `JUDGING.md` ground-truth-first judging (one
judge per question; Q9/baseline and Q16/jevwired re-run once after transient
harness classifier blocks; the Q2/jevwired judge substituted equivalent
read-only GitHub API reads during a classifier outage — noted, ground truth
still independently verified).

| Arm | Jev protocol | Answers | Verdicts |
|---|---|---|---|
| `octojev-p1` ("WITHOUT") | optional observed-state triggers → **0 Jev calls** | `answers/octojev-p1.md` | `judge/` |
| `jevwired-p1` ("WITH") | mandatory scout (3+ row fan-outs) + `hallucination_gate` per answer | `answers/jevwired-p1.md` | `judge-jevwired/` |

## Quality

| Metric | WITHOUT | WITH | Δ |
|---|---|---|---|
| **Correctness** | **279/300 (93.0%)** | **285/300 (95.0%)** | **+6** |
| Research depth | 141/150 | 143/150 | +2 |
| Workflow | 146/150 | 149/150 | +3 |
| Distribution | 18×10 · 9×9 · 2×8 · **1×2** | 18×10 · 10×9 · 1×8 · 1×7 | worst case 2 → 7 |

## Classic-LLM (worker) tokens

| Meter | WITHOUT | WITH | Δ |
|---|---|---|---|
| API turns | 342 | 476 | +39% |
| Output tokens | 122,228 | 172,922 | +41% |
| Fresh input (in + cache-creation) | 749,515 | 1,025,411 | +37% |
| Cache-read | 62.9M | 114.3M | +82% |
| Wall clock | 24.7 min | 34.5 min | +40% |

## Jev tokens (separate meter, $0.042/M input, no output charge)

| | WITHOUT | WITH |
|---|---|---|
| Calls | 0 | 32 (30 gates + 1 re-gate after block + 1 scout) |
| Input / output tokens | 0 / 0 | 45,019 / 2,689 |
| Cost | $0 | **≈ $0.0019** |

## What Jev concretely contributed (WITH arm)

1. **`hallucination_gate` prevented the run's only catastrophic error.** On Q14
   — the exact question the WITHOUT arm failed at 2/10 (dep-section misread
   across an elided view) — the gate **blocked** (soft evidence-anchor tie),
   the runner narrowed the claim to what the evidence grounded, re-gated once
   (grounded=0.97), and the judge scored it **10/10, zero errors**. Judged +8
   directly attributable to the gate.
2. **Gate probabilities were calibrated.** The two lowest grounded scores that
   still proceeded (Q28 at 0.78, Q17 at 0.80) include **Q28 — the WITH arm's
   weakest judged answer (7/10)**. The gate flagged in advance what the judge
   later penalized.
3. **`jevScout` fired once and paid**: Q13 ranked 11 candidate PR rows in one
   batched call and fetched only the 1 marked row, skipping 10 fetches.
4. 29/30 gates said "proceed" — on a question set this targeted, Jev is mostly
   cheap insurance confirming grounded work.

## Verdict

Jev's own bill is a rounding error (~$0.002). The real price of the mandatory
checkpoints is **~40% more worker tokens and wall clock** (packet authoring +
extra turns). The measured return: **+2pp correctness, worst-case failure
lifted from 2/10 to 7/10, one catastrophic error provably prevented, and
calibrated risk flags**. Use the wired checkpoints when wrong answers are
expensive; use the observed-state-trigger (effectively no-Jev) mode when speed
and cost dominate. Single pass, single judge per question — directional; a
contested delta needs the historical order-swap/second-judge treatment.

## Per-question correctness (paired)

| Q | WITHOUT | WITH | Δ |
|---|---|---|---|
| Q1 | 10 | 10 | 0 |
| Q2 | 9 | 9 | 0 |
| Q3 | 10 | 10 | 0 |
| Q4 | 9 | 10 | **+1** |
| Q5 | 9 | 9 | 0 |
| Q6 | 10 | 10 | 0 |
| Q7 | 10 | 10 | 0 |
| Q8 | 10 | 9 | **-1** |
| Q9 | 10 | 10 | 0 |
| Q10 | 9 | 10 | **+1** |
| Q11 | 10 | 10 | 0 |
| Q12 | 10 | 9 | **-1** |
| Q13 | 10 | 10 | 0 |
| Q14 | 2 | 10 | **+8** |
| Q15 | 9 | 9 | 0 |
| Q16 | 10 | 10 | 0 |
| Q17 | 10 | 10 | 0 |
| Q18 | 10 | 10 | 0 |
| Q19 | 8 | 9 | **+1** |
| Q20 | 10 | 9 | **-1** |
| Q21 | 10 | 10 | 0 |
| Q22 | 10 | 10 | 0 |
| Q23 | 9 | 9 | 0 |
| Q24 | 10 | 10 | 0 |
| Q25 | 9 | 9 | 0 |
| Q26 | 10 | 10 | 0 |
| Q27 | 10 | 9 | **-1** |
| Q28 | 9 | 7 | **-2** |
| Q29 | 9 | 10 | **+1** |
| Q30 | 8 | 8 | 0 |
