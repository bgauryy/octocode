# Report template — scored runs under the benchmark output directory

Canonical structure for top-notch, trendable benchmark results. A run that
deviates from this template is not comparable to prior runs and must say why.
Machine-readable twin: `kpi.json` per [`../schemas/kpi.schema.json`](schemas/kpi.schema.json).

## Files

```
packages/octocode-benchmark/output/<run-name>/
├── manifest.md           # frozen prompts/oracles, model/settings, SHAs, versions, budgets
├── SUMMARY.md            # Goal / KPI / Loop level / Checks run / Verdict (validate: node packages/octocode-benchmark/scripts/loop-report.mjs --input <run>/SUMMARY.md; score=1 required only for an ACCEPT/WIN verdict)
├── kpi.json              # machine rollup — dashboards + future runs diff against this
├── <suite>.md            # one report per suite (sections below)
├── answers/<suite>/<arm>/answer_QNN.md
└── logs/<suite>/<arm>/QNN.jsonl
```

## SUMMARY.md — headline first

1. **Verdict line per suite** (one sentence, pre-registered decision rule
   applied): e.g. `octocode-vs-gh: WIN — 1.00 vs 0.67 uncontaminated correctness at 0.098× bytes, no guardrail regressed.`
2. **Headline table** — the whole run at a glance:

| Suite | Verdict | nEligible / k | tokenSource | Correctness B vs A [95% CI] (uncontaminated) | Precision B vs A | Recall B vs A | Flow B vs A | median VRPT B vs A [95% CI] | Tokens B/A | Raw/read bytes B/A | Calls B/A | False-conf Δ | Contaminated / invalid Qs |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|

Verdict vocabulary: `WIN` / `LOSS` / `TIE` require `tokenSource=runner`, `nEligible ≥ 12`, `k ≥ 3`, and non-overlapping 95% CIs on the deciding metric; otherwise use `DRAFT — tokens not captured` or `INCONCLUSIVE (underpowered, n=<N>)`. Never claim WIN from estimated tokens (SCORING.md token-source gate).

3. **Provenance block**: date, `subjectSha`, model + step budget, baseline
   versions (`gh`/`rtk`/`ast-grep`), oracle verification date, solvers per arm
   (k), and whether pass^k was met.

## Per-suite report — four required sections

### 1. Tokens usage (the cost KPI)

Per-question table, one row per question, columns per arm:

| Q | A raw/read bytes | A calls/turns | B raw/read bytes | B calls/turns | B/A raw | B/A read | notes |

Plus the per-arm authoritative block: runner-reported `agentTokens`
(input / output / cache_read / cache_write when available), wall-clock,
total tool uses.
State the estimator used wherever runner tokens are missing (chars/4), label it
as an estimate, and never compare one arm's raw bytes with the other's read
bytes. Include total, mean, median, and B/A ratio for runner tokens, raw bytes,
read bytes, calls, turns, and wall time.

### 2. Questions (the correctness KPI)

| Q | Topic | Contaminated (C≥1.0)? | A correctness | B correctness | toolUsed (B) |

- Correctness is the mean judge score 1–10 from stage 1 (mean of 3 parallel judges).
- Contaminated questions are shown, flagged, and **excluded from the primary
  mean** — never silently dropped.
- `toolUsed` = trajectory layer: did B exercise the question's
  `capabilityPoint`? "Right answer without the tool" is reported as a finding.

### 3. Scores and flow (judge, blind — protocol: [JUDGING.md](JUDGING.md))

| Q | A C/P/R (mean of 3 judges) | B C/P/R | A judgeStd | B judgeStd | A flow | B flow | note |

C/P/R = Correctness / Precision / Recall, each 1–10 (mean of 3 parallel stage-1 judges).
`judgeStd` = score stdev across the 3 judges — flag rows with judgeStd > 1.5 as low-confidence.
Correctness stays in section 2 — do not double-count.

**Arm rollup (required):** per arm report `nEligible`, `pass@1`, `pass^k`,
`meanCorrectness [95% CI]`, `medianVRPT [95% CI]`, `medianVR`, and `tokenSource`.
A verdict-eligible run has `tokenSource=runner`, `nEligible ≥ 12`, `k ≥ 3`, and
reports both bootstrap CIs; state the resample count. Overlapping CIs → `TIE /
underpowered`.

Flow is stage 2 (sealed logs). Include when the stage ran; otherwise state "flow: not scored".

### 4. Guardrails & validity (the honesty section)

- False-confidence count per arm (must not increase vs prior run).
- Dropped / timed-out questions, explicitly listed.
- Control-arm scores per question (basis of contamination flags).
- Oracle drift found during judge re-verification (what moved, what was
  corrected — corrections go into ground-truth `verification` blocks, never
  into mid-run question edits).
- For parity suites: per-call data-parity result (any divergence = surface bug,
  reported regardless of scores).

## Presentation rules (best practice)

- **Ratios over absolutes** for cost (`B/A bytes`, `B/A tokens`, `B/A calls`);
  absolutes live in the tables, ratios in the verdicts.
- **Report medians alongside means** for bytes/tokens (one 500KB dump skews a
  mean); pass^k alongside pass@1 for correctness.
- **Keep accountings separate:** runner tokens, raw bytes, and read bytes answer
  different questions and each gets its own B/A ratio.
- **No verdict from a draft suite** — oracle verification status is printed
  next to every verdict.
- **Losses and ties are reported as prominently as wins** (e.g. "gh-rtk:
  correctness TIE, win is 0.24× bytes"). A benchmark that only ever reports
  wins is marketing, not measurement.
- Every number in SUMMARY.md must be reproducible from `kpi.json` + `logs/`.
