# VRPT — Verified Result Per Token (REQ v3)

One-number efficiency metric: verified result quality per 100k tokens.
Three judge scores, harmonic mean, divided by tokens.

## Per question, per arm

Judge scores each candidate on **three dimensions, each 1–10**:

| Score | Range | What it measures | 1 | 10 |
|---|---|---|---|---|
| **Correctness** (C) | 1–10 | Did the answer get it right? | Wrong or fabricated | Every clause correct, decisive evidence verified |
| **Precision** (P) | 1–10 | No false outputs / hallucinations? | Almost everything stated is wrong | Nothing stated is incorrect |
| **Recall** (R) | 1–10 | No missing info? | Missed almost everything important | Found everything important |

```
VR_q   = harmonic_mean(C, P, R) / 10   =   3 / (10/C + 10/P + 10/R)
VRPT_q = 100_000 × VR_q / T_q
```

`T_q` = per-question runner tokens; fallback: `(readBytes_q + answerChars_q) / 4` — record which as `tokenSource`.

**Hard rule:** `Correctness ≤ 2` → `VR = 0`. Fabricated or completely wrong answers earn no efficiency credit at any token count.

**Why harmonic mean:** lopsided failure on any dimension collapses the score — Precision=1 (hallucinated everything) with C=R=10 gives VR=0.25. Matches F1/RAGAs standard for averaging precision and recall rates.

## How anchor verification informs scores

The judge still fetches ≥3 decisive anchors from a surface outside both arms. The check results feed the scores — they are not counted mechanically into the formula:

- A fabricated anchor (FAIL) → drives Correctness ≤ 2 (VR = 0) and Precision toward 1–3.
- Stale line numbers but correct substance (DRIFT) → small Precision penalty, not fabrication.
- Missing whole topics the question required → Recall toward 1–4.
- Absence claims ("no integration exists") verified with multiple independent probes → Recall credit.

## Aggregation — per arm

- **Primary: `median(VRPT_q)`** across uncontaminated, completed questions. Never ratio of token totals.
- Report `mean(VRPT_q)` and `median(VR_q)` alongside. All three go in `kpi.json`.

### Sample size, uncertainty, and effect size (pre-registered)

These banks are small (github research-v2 = 14 Qs, ~12 uncontaminated;
ast-grep = 10). A bare median over ~12 questions is fragile, so every scored run
MUST report uncertainty, not just point estimates:

- **Report `nEligible`** (valid, uncontaminated, completed questions feeding the
  primary aggregate) next to every headline number. It goes in `kpi.json`
  (`armRollup.eligibleQuestions`) and the SUMMARY headline.
- **Bootstrap 95% CI** (≥10,000 resamples over the per-question values) on
  `median(VRPT)` and on `mean(Correctness)` for each arm; report the CI in
  `kpi.json` (`medianVRPTci: [lo, hi]`, `meanCorrectnessCi: [lo, hi]`) and the
  suite report. A **WIN** requires the arms' 95% CIs to be **non-overlapping**
  on the deciding metric; overlapping CIs are reported as `TIE / underpowered`,
  never as a win.
- **Effect size:** report the median-difference (and its bootstrap CI), not only
  the B/A ratio, so a "2×" headline on tiny absolute numbers is visible as such.
- **Low-power label (hard):** with `nEligible < 12` a run is **descriptive
  only** — it MAY report medians + CIs but MUST set the verdict to
  `INCONCLUSIVE (underpowered, n=<N>)` and may not claim WIN/LOSS. Raising power
  means more eligible questions or more solvers (k), never re-reading the same
  answers.
- **Reliability:** report `pass@1` (mean correctness) **and** `pass^k` (all k
  solvers score Correctness ≥ 8); a verdict needs `k ≥ 3`.


**Efficiency verdict rule (pre-registered):** arm X wins efficiency iff all four hold:
0. `tokenSource == "runner"` for **both** arms (estimated-only → no verdict, DRAFT)
1. `median(VRPT_X) > median(VRPT_Y)`
2. `median(VR_X) ≥ 0.6`
3. mean Correctness of X is not lower than Y's by more than 1.0 point

VRPT never overrides a correctness loss. Correctness tie + better VRPT = `correctness TIE / efficiency WIN`.

## Tool-property KPIs (report alongside VRPT)

| KPI | Definition | Why it matters |
|---|---|---|
| `readTokensMedian` / `readTokensP90` | p50 and p90 of per-question read tokens | p90 = tail cost; agents die on worst-case blowouts |
| `readTokensCV` | stdev/mean of per-question read tokens | predictable budgets are plannable |
| `rawBytesMedian` / `rawBytesP90` | raw emission before solver filtering | pure tool property |
| `signalRatioMedian` | read/raw per question (median) | 1.0 = nothing wasted |

## Lineage

Supersedes REQ v2 (`C×(Q/5)×E×(1−H)` with ternary correctness, 1–5 quality, anchor ratios):
- v2 mixed objective anchor counts (E, H) with subjective quality (Q) in one formula — hard to calibrate
- v2 required judges to count anchors mechanically rather than score holistically
- v3: three 1–10 scores matching established Precision/Recall/F1 semantics (RAGAs, FActScore); anchor verification informs scores, not the arithmetic

Prior REQ v2 results stay valid within their own runs; do not mix v2 and v3 rows in the same ledger.
