# Measurements — how to measure tokens and compute the comparison

Load when computing or reporting token costs and the VR/VRPT comparison for a run.
Authoritative math: [`../../../SCORING.md`](../../../SCORING.md); the schema field names
are in [`../../../schemas/kpi.schema.json`](../../../schemas/kpi.schema.json).

## 1. How to measure tokens

VRPT is a **per-question** metric, so its denominator is **per-question tokens** —
never a whole-trial total split after the fact. Pick the denominator per question,
per arm, in this priority order and record which tier you used in `tokenSource`:

| Tier | `tokenSource` | Source | Use when |
|---|---|---|---|
| **Runner tokens** (primary; verdict-eligible) | `"runner"` | Provider's per-question / per-turn counters: input + output + cache-read + cache-write | The harness exposes tokens at question granularity |
| **Labeled estimate** (`VRPT-est`, DRAFT only) | `"estimated"` | `(readBytes_q + answerChars_q) / 4` for that question — `estTokens` | Runner exposes only a whole-trial total, or nothing |

> **Token-source gate (hard):** VRPT's denominator is runner tokens. If either
> arm is `"estimated"`, drop **both** to `VRPT-est` and mark the suite **DRAFT
> for efficiency** — report direction only, never a WIN. Byte proxies omit
> system-prompt / tool-schema / cache tokens and conflate tool output with the
> answer, flattering terse-payload arms. Confirm the runner-token sensor at
> preflight (SKILL.md run sequence step 1), not mid-run.

Rules:
- **Never split a whole-trial runner total across questions by hand** — that fabricates
  per-question precision the counter never had. If only a trial total exists, use the
  per-question **estimate** for per-question VRPT, and report the runner trial total
  separately in `agentTotals.<suite>.<arm>.agentTokens` as the authoritative whole-trial KPI.
- **Never mix tiers across arms** in the same suite. If arm A has runner tokens and arm
  B only has estimates, drop both to `"estimated"` so the comparison is apples-to-apples.
- If a value is truly unavailable, write `Unavailable` — never fabricate a number.

## 2. Byte accounting (separate from tokens — never mix)

For every tool call log entry `{id, tool, exitCode, ms, rawBytes, readBytes}`:

- `rawBytes`: complete stdout/tool payload **before** any solver-side filtering.
- `readBytes`: bytes actually delivered to the solver **after** filtering (≤ rawBytes).
- Per question, sum each: `rawBytes_q`, `readBytes_q`.
- Signal ratio `= readBytes_q / rawBytes_q`. Never compare one arm's raw bytes to
  another arm's read bytes.

## 3. Per-question required measurements

| Field | Meaning |
|---|---|
| `runnerTokens` | per-question runner tokens, or `null` when estimated |
| `tokenSource` | `runner` \| `estimated` |
| `estTokens` | `(readBytes_q + answerChars_q)/4` — always compute it, even when `runner` is used, as a cross-check |
| `rawBytes` / `readBytes` | per-question sums (§2) |
| `calls` | tool invocations for this question, including failures/retries/pagination |
| `wallClockSec` | elapsed time for this question |
| `correctness`, `precision`, `recall` | 1–10 judge scores from stage 1 (SCORING.md) |

## 4. The comparison algorithm (do exactly this)

For each question `q`, each arm:

```
tokens_q = runnerTokens_q  if tokenSource=runner  else estTokens_q

C = correctness_q / 10      # judge score 1-10, normalized
P = precision_q / 10        # judge score 1-10, normalized
R = recall_q / 10           # judge score 1-10, normalized

VR_q = 3 / (1/C + 1/P + 1/R)        # harmonic mean, 0..1
# Hard rule: if correctness_q ≤ 2 → VR_q = 0

VRPT_q = 100_000 * VR_q / tokens_q  # verified points per 100k tokens
```

Hard rules (from SCORING.md — do not deviate):
- `Correctness ≤ 2` sets `VR = 0` for that question — fabricated or completely wrong.
- Harmonic mean is applied once — never switch to arithmetic or geometric.

Then aggregate **per arm**:

```
medianVRPT = median over questions of VRPT_q     # PRIMARY efficiency aggregate
meanVRPT   = mean over questions of VRPT_q        # reported alongside
medianVR   = median over questions of VR_q        # VR floor gate
```

- **Primary is `medianVRPT`. Never use a ratio of token totals** — one 595KB trial or
  one 2.8KB cheap question flips a totals-based headline. (Backtested on
  `2026-08-03-cross-repo-draft`: totals said B pays 0.49× per point; per-question
  medians said 0.93× — a near-tie. Report the median.)
- Report **B/A ratios of the medians**, and of the means, explicitly.
- Only **uncontaminated, valid, completed** questions feed the primary aggregates
  (`eligibilityRule: taskStatus=valid && contaminated=false && arm.status=completed`).
  A trial that broke the hard `maxToolCalls` cap, or (gh-rtk Arm A) read a raw
  `gh` payload > 50 KB without `rtk` shaping, is `taskStatus: invalid` — re-run
  it; do not let it into the aggregates.

### Uncertainty and power (required for a verdict)

- Record `nEligible` (= `eligibleQuestions`) beside every headline number.
- Bootstrap 95% CIs (≥10k resamples) on `median(VRPT)` and `mean(Correctness)`
  per arm → `medianVRPTci`, `meanCorrectnessCi`. A WIN needs the arms' CIs to be
  **non-overlapping** on the deciding metric.
- With `nEligible < 12`, the run is descriptive only: verdict
  `INCONCLUSIVE (underpowered, n=<N>)`, never WIN/LOSS.
- Report `pass@1` and `pass^k`; a verdict needs `k ≥ 3`.

## 5. Efficiency verdict rule (pre-registered)

Arm X wins efficiency **iff** all hold:
0. `tokenSource == "runner"` for both arms (estimated-only → DRAFT, no verdict), **and**
1. `median(VRPT_X) > median(VRPT_Y)`, **and**
2. `median(VR_X) ≥ 0.6` (VR floor — below it, no efficiency win at any token count), **and**
3. mean Correctness of X is not lower than Y's by more than 1.0 point (1–10 scale), **and**
4. `nEligible ≥ 12`, `k ≥ 3`, and the arms' 95% CIs on `median(VRPT)` do not overlap.

VRPT never overrides a correctness loss. A correctness tie + better cost is
`correctness TIE / efficiency WIN`, not an outcome win.

## 6. Tool-property KPIs (report alongside VRPT)

Computed over per-question **read tokens** (`readBytes_q / 4`) and raw bytes:

| KPI (`toolPropertyKPIs`) | Definition |
|---|---|
| `readTokensMedian` / `readTokensP90` | median and p90 of per-question read tokens (tail cost) |
| `readTokensCV` | stdev/mean of per-question read tokens (budget predictability) |
| `rawBytesMedian` / `rawBytesP90` | raw emission before solver filtering — pure tool property |
| `signalRatioMedian` | median of `readBytes_q/rawBytes_q` (1.0 = nothing wasted) |

## 7. Stdout

Emit per question after judging, then the arm rollup:
```
[ORC] Q<NN> vr_A=<N> vr_B=<N> vrpt_A=<N> vrpt_B=<N> tokens_A=<N> tokens_B=<N>
[ORC] KPI medianVRPT_A=<N> medianVRPT_B=<N> medianVR_A=<N> medianVR_B=<N> BoverA=<N>
```
