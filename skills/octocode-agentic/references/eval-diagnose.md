Load when eval results fail, fluctuate or look too good, or when reporting what a change proved.

# Diagnose and report eval results

## Diagnose failures

Read transcripts before trusting any score, not only a suspicious one. Results fail, fluctuate, or look too good: check the measurement before the solver. After Action Review: intended (task contract); happened (use the actual input, result and deciding trace; reproduce under comparable conditions, including retries); why different (first violated contract; test a rival; findings can overlap); next (the owner and fix in the table).

| Cause | Owner | Look for or check | Next |
|---|---|---|---|
| Task | task owner | unsolvable from worker-visible inputs, or hidden tests need an unstated path, API, phrasing | clarify; new dataset version |
| Reference or test | grader owner | rejects valid alternatives, accepts deliberately wrong outputs, ignores final state | fix, validated on independent labels |
| Judge | evaluator | order swaps, domain labels, evidence checks, Unknown and error rates | [recalibrate](eval-grading.md#calibrate-a-judge) or pin |
| Infrastructure or fixture | harness owner | control task, clean replay, resource and timeout logs | restore reproducibility; new environment record; paired rerun |
| Leakage or grader exploitation | controller | exports, tool reads, history, shared state, answer retrieval | invalidate evidence, close access, rotate exposed final tasks, rerun in a verified clean lab |
| Solver defect | developer | sound task and grader, trace violates a supported requirement | patch the smallest general mechanism, harness unchanged |
| Sampling noise, thin coverage | — | paired task-level uncertainty, per-slice results, infrastructure covariance | follow the frozen sampling plan or report Inconclusive |

- Grader-only fix: regrade sealed outputs if they hold all required evidence and the task is unchanged. Question, fixture, worker context, or environment changed: rerun both arms; version the before/after labels.
- Resource limits: record guarantees and hard limits; measure whether more headroom removes crashes or enables stronger search.

## Claim and report

- Editorial review: freeze rating dimensions and anchors first; label the rating subjective, not a measured gain.
- Never invent sealed-test scores or claim release acceptance from reading prose.
- Separate source review, maintenance checks, development measurements, sealed comparisons; flag unmeasured behavior.
- Explain stops or direction changes that affect interpretation; record lessons only when results support them.
- After the verdict, feed failures to [case building](eval-cases.md#cases-from-failures).
- Done when the report states the verdict, the conditions, each guardrail result, the denominators and what was not measured.
