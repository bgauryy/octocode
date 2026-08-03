# Comparison Benchmarks — Methodology

Head-to-head suites: same LLM, same bank questions, same budget — the only
variable is the **tool provider / surface**. This answers the one question that
matters: *does an agent solve real code tasks better with Octocode than with
the alternatives — and at what token cost?*

**Status:** three active v2 suites, **NOT YET SCORED** — see
[`results/`](results/). v1 suites and their evidence were removed from the
tree on 2026-08-03 and are not comparable with v2.

**Metric:** [VRPT (REQ v3)](SCORING.md) = `harmonic_mean(Correctness, Precision, Recall) / 10`
per 100k tokens — three judge scores 1–10: correctness (got it right?), precision
(no false outputs?), recall (nothing missing?); harmonic mean collapses on any
lopsided failure; primary aggregate is per-question **median**; `VR ≥ 0.6` floor.

| Suite | Arm A (baseline) | Arm B (treatment) | Bank (questions) | Stresses |
|---|---|---|---|---|
| [octocode-vs-gh](compare/octocode-vs-gh/) | `gh` CLI only | Octocode **MCP**, remote GitHub tools only | [github/research-v2](questions/github/research-v2/) (Q1–Q14) | GitHub research: code search, bounded file fetch, PRs, history, structure |
| [octocode-vs-gh-rtk](compare/octocode-vs-gh-rtk/) | `rtk` + `gh` CLI | Octocode **MCP**, remote GitHub tools only | [github/research-v2](questions/github/research-v2/) (Q1–Q14) | Same bank vs a token-optimized baseline (output-shaping ablation) |
| [octocode-vs-ast-grep](compare/octocode-vs-ast-grep/) | `ast-grep` CLI | Octocode **CLI**, local tools | [local-code/ast-grep-react-v2](questions/local-code/ast-grep-react-v2/) (Q1–Q10) | Structural/AST parity + beyond-AST (LSP identity, reachability, outlines) |

External suites (github subject) verify oracles outside both arms; local
suites (local-code subject) pin a frozen checkout and recompute counts at run
time. Each question names its differentiator in the bank's `ground-truth.json`
(`capabilityPoint` or suite equivalent) — separating outcome correctness from
tool-use attribution.

Each suite folder has:

- `README.md` — the ONLY suite file: arm boundaries, which bank questions to
  run, corpus, oracle status, and suite-specific gotchas. `questions.md`
  (solver-facing, frozen) and `ground-truth.json` (judge-only) live once in the
  suite's canonical bank under `questions/<subject>/<bank>/`.
- results ledger: tracked at [`../results/<suite>.md`](results/) (latest-first, or a `NOT YET SCORED` stub).

Shared run procedure: [`INSTRUCTIONS.md`](INSTRUCTIONS.md). Canonical
report shape: [`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md). Do not copy those
contracts into each suite; suite instructions are overlays only.

## The three arms (not two)

A tool-provider A/B needs a **control** or the result is unattributable — a
strong LLM can answer famous-repo questions from memory, scoring both tooled arms
high regardless of tool quality (construct-validity failure).

| Arm | Tools | Role |
|---|---|---|
| **Control (C)** | none — LLM answers from memory only, no tool calls | Contamination detector. If C already scores high, keep the row visible but exclude it from the primary mean; replace it only between runs. |
| **Baseline (A)** | the suite's baseline toolchain (`gh` / `rtk`+`gh` / `ast-grep`) | The thing Octocode must beat. |
| **Treatment (B)** | The Octocode surface fixed by the suite | The system under test. |

Run C first. A question earns its place only if `C < A,B` — i.e. tools actually
matter for it. Contaminated questions remain visible but are excluded from the
primary mean; do not silently down-weight them.

## Method

1. **Freeze the harness.** Same model, prompts, step budget. Arm A gets only its
   baseline toolchain; Arm B gets only the Octocode surface named by the suite;
   Arm C gets no tools. Evolve the suite only *between* runs, never mid-run.
   **Budgets are hard, not advisory:** each question's `task.budget.maxToolCalls`
   is a hard cap — any trial (any arm) that exceeds it is `taskStatus: invalid`,
   excluded from aggregates, and re-run. This is symmetric across arms and keeps
   the comparison a tool property, not a solver-discipline artifact.
   For the **`gh`+`rtk`** baseline specifically, `rtk` is a third-party stdout
   shaper that adds no research source (pin its version in `manifest.md`,
   `baselines.rtk`); any Arm A tool call whose `rawBytes` > 50 KB MUST be shaped
   through `rtk` before it reaches the solver, or the trial is invalid and
   re-run. Without this rule the baseline's byte cost measures whether the
   solver *remembered* to shape output, not the tool — see
   [`compare/octocode-vs-gh-rtk/README.md`](compare/octocode-vs-gh-rtk/README.md).
2. **Independent solvers, aggregated.** ≥3 solvers per arm for a **publishable
   verdict** — report **pass@1 mean correctness** (capability) **and** **pass^k**
   (reliability). k=1 runs are valid draft/exploratory evidence but must be
   labeled and cannot verdict.
3. **Log every research call** as `{id, cmd|tool, exit, ms, rawBytes,
   readBytes}`. Runner token counters belong to the whole trial, not an
   individual command. These are deterministic anchors for cost and trajectory
   grading.
4. **Grade in fresh context, blind.** The judge is a separate agent that never
   saw the solve and does not know which arm produced an answer. A verifier
   sharing the executor's context is not independent.

## Grading

The full judge protocol — blinding, anchor verification, scoring fields,
question-type checklists, and the two stages (3 parallel blind judges for
outcome → sealed-log flow) — lives in **[JUDGING.md](JUDGING.md)**; the math in
**[SCORING.md](SCORING.md)**. Two principles worth restating here:

- **Anchor-grounded**: the judge fetches ≥3 decisive anchors from a surface
  outside both arms; scores flow from the anchor results, not from counts.
  Any FAIL anchor drives Correctness ≤ 2.
- **Attribute the win**: each question carries a `capabilityPoint`; stage 2
  records whether the arm actually exercised it. A correct answer reached
  without the differentiating tool still counts for outcome but is reported as
  "answered without the tool" — itself a finding.

## Metrics (per question, per arm)

| Metric | Kind | Meaning |
|---|---|---|
| correctness (pass@1 mean) | primary | mean Correctness score (1–10) across uncontaminated questions |
| reliability (pass^k) | primary | all k solvers score Correctness ≥ 8? |
| precision | secondary | judge score 1–10: nothing wrong stated / no hallucinations |
| recall | secondary | judge score 1–10: nothing important missing |
| VRPT | headline efficiency | `100k · harmonic_mean(C,P,R)/10 / tokens`, per-question **median** |
| flow | optional | `1–5` — stage-2 sealed-log trajectory grade where run ([JUDGING.md](JUDGING.md)) |
| tokens | guardrail (untunable) | whole-trial runner input/output/cache tokens; labeled estimator only when unavailable |
| time (wall-clock) | guardrail | seconds to answer |
| calls | guardrail | research tool invocations, including failures and pagination |
| turns | guardrail | assistant turns, including bookkeeping-only turns |
| false-confidence | guardrail (untunable) | wrong answer asserted as proof — must not increase |
| tool-used | trajectory | did the arm use the differentiating tool? |
| control-lift | validity | `B − C` and `A − C`; if ~0 the question is contaminated |

**Decision rule (pre-register before running).** Report dimensions separately:
an **outcome win** requires higher mean correctness on uncontaminated questions;
an **efficiency win** requires higher **median VRPT** with `median VR ≥ 0.6` and
mean Correctness not lower than baseline by more than 1.0 point ([SCORING.md](SCORING.md)).
**Verdict-eligibility gates (hard):** any WIN/LOSS requires `tokenSource=runner`
(estimated-only tokens → `DRAFT`, report `VRPT-est` for direction only),
`nEligible ≥ 12`, `k ≥ 3`, and **non-overlapping 95% CIs** on the deciding
metric — otherwise `INCONCLUSIVE (underpowered)`.
An overall **WIN** requires both dimensions and no guardrail regression. A correctness tie
plus better cost is `correctness TIE / efficiency WIN`, not an outcome win.
False confidence must not increase. Report dropped, timed-out, and contaminated
questions explicitly — silent truncation reads as coverage that did not happen.

## Run output contract — make it a KPI

Execution steps: [`INSTRUCTIONS.md`](INSTRUCTIONS.md). Canonical report
structure: [`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md)
(headline table, four required per-suite sections, presentation rules —
ratios+medians, losses reported as prominently as wins).

Every scored run writes
`packages/octocode-benchmark/output/<run-name>/` containing:

- one report per suite, with **four required sections per suite** so runs are
  comparable and trendable:
  1. **Tokens usage** — per-question table: bytes of tool output per arm,
     estTokens (chars/4), calls; plus per-arm runner-reported agent tokens and
     wall-clock (authoritative tokens KPI).
  2. **Questions** — per-question table: topic, contamination flag, per-arm
     Correctness score (mean of 3 judges, 1–10).
  3. **Scores and flow (judge)** — per-question C/P/R means and judgeStd from
     stage-1 parallel judges; stage-2 sealed-log flow score (1–5) where run.
  4. **Guardrails & validity** — false confidence, control/contamination,
     dropped questions, oracle drift, and parity findings.
- `kpi.json` conforming to
  [`../schemas/kpi.schema.json`](schemas/kpi.schema.json) — the
  machine-readable rollup dashboards and future runs diff against.
- `SUMMARY.md` with Goal / KPI / Loop level / Checks run / Verdict sections,
  validated by [`scripts/loop-report.mjs`](scripts/loop-report.mjs)
  (`node packages/octocode-benchmark/scripts/loop-report.mjs --input
  output/<run>/SUMMARY.md`; `--self-test` for the validator itself). score=1
  (all gates pass) is required only for an ACCEPT/WIN verdict; the validator
  additionally blocks a claimed WIN whose tokens are estimated-only, whose k=1,
  or whose oracle is `UNVERIFIED_DRAFT`.

Solver arms return JSON per
[`../schemas/solver-output.schema.json`](schemas/solver-output.schema.json);
questions seal per
[`../schemas/questions-input.schema.json`](schemas/questions-input.schema.json).
Contract fixture: [`../fixtures/compare-run-example/`](fixtures/compare-run-example/).
Real scored runs write to gitignored
`packages/octocode-benchmark/output/<run-name>/`.

## Related metrics and prior art

VRPT shares a ratio skeleton with published token-efficiency metrics but differs
in the numerator, aggregation, and verdict contract.

| Metric | Formula | Difference from VRPT |
|---|---|---|
| **ApT** (iMAD, AAAI 2025) | `Acc × 100k / tokens_per_question` | Binary accuracy numerator only; no quality, evidence, or hallucination factor; no floor gate; mean-aggregated. The paper itself flags the gap: high ApT at low accuracy is a bad trade — the exact problem the composite numerator and `VR ≥ 0.6` floor gate address. |
| **OckScore** (OckBench, 2024) | `accuracy − 10 × log(tokens/10k)` | Log-penalty, not a ratio; no composite numerator; applied to a filtered 200-instance differentiation set. |
| **Cost-of-Pass** (Stanford, 2025) | `cost / P(correct)` | Economic dual (cost per correct answer); binary correctness only; no quality, evidence, or hallucination factor. |
| **CLEAR framework** (2024) | Five separate dimensions (cost, latency, efficacy, assurance, reliability) | Identifies the same problem space but keeps dimensions separate — no single composite ratio or per-question efficiency score. |
| **T\*** / tokens-per-success | `total_tokens / successful_tasks` | Inverted binary accuracy; whole-trial denominator; no composite quality. |

**What ApT validates:** the per-question token denominator (never split a whole-trial total
across questions) — ApT arrived at the same design independently (arxiv 2511.11306).

**What VRPT adds over all of them:**
1. Harmonic mean `VR = harmonic_mean(C, P, R) / 10` — a lopsided failure on any dimension collapses the score; a hallucinated-but-correct answer (Precision=1) with C=R=10 gives VR=0.25, same as a near-miss.
2. Pre-registered `VR ≥ 0.6` floor gate — efficiency verdict is invalid below minimum quality.
3. Median-primary aggregation — resists outlier inflation from a single cheap question (backtested: totals said B pays 0.49× per point; per-question medians said 0.93× — a near-tie, not a win).

**References:**
- ApT / iMAD: <https://arxiv.org/abs/2511.11306>
- OckBench: <https://arxiv.org/abs/2511.05722>
- Cost-of-Pass: <https://arxiv.org/abs/2504.13359>
- CLEAR framework: <https://arxiv.org/abs/2511.14136>

## Validity gates

- **Runnable-sensor gate.** A suite is a **ship-gate only when every oracle is
  independently verified and frozen.** Per-bank status lives in each bank's
  `ground-truth.json` (`oracleStatus` + `verification` block) — re-verify
  time-sensitive facts before every scored run. Do not report a "win" from a
  suite whose bank oracle is unverified.
- **Ground truth outside both arms.** Verify with a method neither arm uses
  (WebFetch `raw.githubusercontent.com` / `api.github.com`; local suites re-read
  the pinned checkout), never by a toolchain grading itself. Parity oracles
  whose oracle is "both tools agree" additionally need an **independent
  third-method spot-check** on a sample (manual/`grep -c`) — agreement alone is
  self-referential (both tools can be wrong the same way).
- **Contamination.** Famous repos/PRs are high-leakage; the control arm detects
  it per question. Prefer targets pinned to commits/PRs *after* the model's
  training cutoff where possible.
  - **Score the control on anchor-level recall, not narrative plausibility.** A
    no-tools answer that names the broad architecture but cites no verified
    file/path/line/PR#/count scores **0** and is **not** contamination —
    guessing the shape of a famous system is not the same as retrieving its
    facts. A question is contaminated only when the control reaches the required
    anchors (`controlCorrectness ≥ 1.0` on the rubric's atomic claims). This is
    the draft's key lesson: the `2026-08-03-cross-repo-draft` control "correctly
    guessed" the architecture on several questions while recalling zero anchors
    — that is signal the oracle must weight by evidence, not a contamination
    flag.
  - **Q6 (express router) and Q10 (axios)** in `github/research-v2` are famous +
    memorizable and stay flagged `contaminated`: kept visible, always excluded
    from the primary mean, never verdict-bearing, until replaced by post-cutoff
    targets in a future curation pass (tracked in the bank `ground-truth.json`).
- **Time-sensitivity.** PR/issue state, line numbers, and counts drift —
  re-verify before a scored run and record the run date.
- **Cheat resistance.** Never edit graders/questions mid-run to move a number
  (REJECT). ~0% on a question → debug the task/grader first, not the tool.
- **Capability vs regression balance.** Keep should-fire (hard) and
  should-not-fire cases (absence traps that reward proving a gap) so honesty is
  scored, not just recall.
