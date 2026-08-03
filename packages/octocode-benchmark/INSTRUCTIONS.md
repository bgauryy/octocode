# Run a Comparison Benchmark

This is the shared how-to for every suite in this directory. Suite `README.md`
files define only the corpus, arm-specific tool boundaries, question map,
oracle status, and known gotchas. The methodology is owned by
[`README.md`](README.md); the report shape is owned by
[`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md).

## 1. Preflight and freeze

Before any solver starts:

1. Create
   `packages/octocode-benchmark/output/<run-name>/manifest.md`.
2. Record UTC time, subject/corpus SHA, exact model and settings, step/time
   budget, solver count `k`, tool versions (including `baselines.rtk` for the
   gh-rtk suite), and oracle-verification date.
2b. **Confirm the token sensor before spending any solver budget.** A
   verdict-eligible run REQUIRES per-question runner tokens
   (`tokenSource: "runner"`). Probe the harness once and record the result in
   `manifest.md`. If per-question runner tokens are not exposed, the run is
   **DRAFT for efficiency**: you may still report `VRPT-est` (byte proxy) for
   direction, but the efficiency verdict is `DRAFT — tokens not captured` and no
   WIN may be claimed (SCORING.md token-source gate). Capture the sensor first —
   do not discover mid-run that the headline metric is unmeasurable.
2c. Confirm the frozen bank hash: `shasum -a 256
   questions/<subject>/<bank>/questions.md` MUST equal the `questionBankHash`
   stored in that bank's `ground-truth.json`. A mismatch means the bank drifted
   — stop and reconcile before running.
3. Freeze the suite's canonical bank (`questions/<subject>/<bank>/questions.md` and `ground-truth.json`) — no edits allowed once any solver starts.
4. Confirm solvers can read `questions.md` but cannot read ground truth, prior
   answers, other arms, or judge artifacts.
5. Run the no-tools control first. Mark `C >= 1.0` questions contaminated and
   exclude them from the primary correctness mean; never merely down-weight
   them without a pre-registered weight. A draft run may run the control
   concurrently (isolated context) or defer it, but cannot produce a verdict
   until the control has run.
6. Freeze questions, rubrics, caps, and verdict rules until the run ends.

A suite with unverified oracles may be exercised as a draft, but it cannot
produce a benchmark verdict.

## 2. Isolate solver trials

- Use a fresh context for every solver trial.
- Use the same model, settings, prompt, and budget across arms.
- Enforce the suite's tool allowlist. Timing and byte-counting shell commands
  are bookkeeping, not research tools.
- **Enforce budgets as hard caps.** Each question's `task.budget.maxToolCalls`
  is a hard cap for every arm: a trial that exceeds it is `taskStatus: invalid`,
  excluded from aggregates, and re-run. Log the invalidation.
- **gh-rtk fairness:** any Arm A tool call whose `rawBytes` > 50 KB MUST be
  shaped through `rtk` before the payload reaches the solver context; an
  unshaped >50 KB read is an invalid trial and is re-run. This removes the
  solver-discipline confound so the byte comparison measures the tool.
- Require exact evidence anchors. Search snippets are discovery evidence, not
  proof. `Unknown` is correct when the budget cannot support a claim.
- Log failed, empty, retried, and paginated calls as well as successful calls.
- Use at least three independent solvers per arm for a publishable reliability
  claim; report pass@1 mean, pass^k, and variance. k=1 is acceptable for
  labeled draft/exploratory runs.

## 3. Write run artifacts

Canonical layout:

```text
packages/octocode-benchmark/output/<run-name>/
├── manifest.md
├── SUMMARY.md
├── kpi.json
├── <suite>.md
├── answers/<suite>/<arm>/answer_Q01.md ... answer_Q<NN>.md
└── logs/<suite>/<arm>/Q01.jsonl ... Q<NN>.jsonl
```

The machine-readable solver output follows
[`../schemas/solver-output.schema.json`](schemas/solver-output.schema.json).
Per-question Markdown answers are the human-readable twin and use:

```markdown
# Q01 — <short title>

## Run metadata
- Arm:
- Model:
- Started / finished (UTC):
- Wall time:
- Runner tokens: <exact counters or Unavailable>
- Tool-output bytes: <raw and read>
- Research calls:

## Answer
<answer every subpart directly>

## Evidence
- `<repo>@<ref>:<path>:<range>` — <claim>

## Gotchas
- <drift, ambiguity, truncation, unsupported premise, or Unknown>
```

Do not store secrets, auth headers, full stdout dumps, or hidden reasoning in
answer files.

## 4. Measure cost without mixing accountings

- **Runner tokens (primary):** whole-trial input, output, cache-read, and
  cache-write counters from the execution provider.
- **When unavailable:** write `Unavailable`; do not fabricate. Report
  final-answer characters/4 and tool-result characters/4 only as labeled
  estimators, tagged `VRPT-est` — an estimated-only run is DRAFT for efficiency
  and cannot produce a WIN (SCORING.md token-source gate).
- **Report `nEligible` and bootstrap 95% CIs** on `median(VRPT)` and
  `mean(Correctness)` per arm; a WIN needs non-overlapping CIs and `nEligible ≥
  12`, else the verdict is `INCONCLUSIVE (underpowered)` (SCORING.md).
- **Raw bytes:** complete stdout/tool payload before solver-side filtering.
- **Read bytes:** bytes actually delivered to or read by the solver after
  filtering. Report raw and read separately; never compare one arm's raw bytes
  with another arm's read bytes.
- **Wall time:** monotonic elapsed time around the whole trial.
- **Calls and turns:** count research calls and assistant turns separately.

Report per-question values, totals, means, medians, and B/A ratios. One large
payload can dominate a mean, so the median is mandatory.

## 5. Judge independently and blind

After all outputs are sealed, randomly map each answer pair to Candidate X/Y
and follow **[JUDGING.md](JUDGING.md)** — the complete protocol: stage 1 (3
parallel blind judges each scoring Correctness / Precision / Recall 1–10, with
≥3 decisive-anchor verifications per candidate on a surface outside both arms;
orchestrator aggregates mean C/P/R + majority winner + judgeStd), stage 2
(flow from sealed logs, last — logs unblind the arms). Scoring math:
[SCORING.md](SCORING.md).

Score the control arm for **contamination only, on anchor-level recall** — a
no-tools answer that names the architecture but cites no verified anchor scores
0 and is not contamination; flag contaminated only at `controlCorrectness ≥ 1.0`
(JUDGING.md control-arm section).

Reveal the arm mapping only after each stage's scores are sealed.

## 6. Report and preserve

Build `<suite>.md`, `kpi.json`, and `SUMMARY.md` from the sealed artifacts using
[`REPORT_TEMPLATE.md`](REPORT_TEMPLATE.md). Apply the pre-registered verdict
rule, report losses and ties as prominently as wins, and list contamination,
timeouts, dropped questions, oracle drift, missing token telemetry, and other
limitations.

Finally refresh the suite's tracked ledger at `results/<suite>.md` (package
root), latest run first. Never
overwrite prior-run history, and never copy raw gitignored artifacts into the
tracked ledger.
