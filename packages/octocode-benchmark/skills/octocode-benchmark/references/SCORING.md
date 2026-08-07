# Scoring (by hand)

Three arms per question — **A** Octocode, **B** gh+RTK, **C** gh+Headroom. **The judge's
written reasoning is the evidence; the number only summarizes it** — a score with no
reasoning behind it is not usable. Each answer is scored on **correctness (0–10), research
depth (1–5), workflow (1–5)** — rubric and per-answer reasoning live in
[JUDGING.md](JUDGING.md) — plus one measured quantity defined here:

- **Total chars through the model** — model-in (tool output pulled into context) **+**
  model-out (commands/args + final answer), per question, from the instrumented log — never
  self-reported.

## Aggregate per question, paired

The **question is the unit**; the anchor and baseline are matched. Do not pool raw outputs and
compare totals — summing weights each question by size, so one heavy question sets the
verdict.

- **Characters:** headline the **geometric mean of per-question ratios** (each baseline ÷
  Octocode) + **median** + **leaner win-rate (sign test)**. Report a pooled sum only with
  its top-contributor share and a leave-one-out.
- **Correctness / depth / workflow** (near ceiling): paired **win/tie/loss + sign test**,
  not mean gaps.

Full method + worked example:
[`aggregation-and-stats.md`](aggregation-and-stats.md).

## Decide, correctness-first

Per pairing (Octocode vs each baseline): if one arm is net strictly more correct (paired
sign test) it wins — a confidently-wrong answer never wins on footprint (see
[JUDGING.md](JUDGING.md)). If correctness is statistically indistinguishable, efficiency
decides by **geometric-mean char ratio**, never the pooled sum alone. Full decision rule,
tests, and worked example: [`aggregation-and-stats.md`](aggregation-and-stats.md).

Two honesty guards that gate any shipping claim:

- **Report uncertainty, not a point.** One pass is a snapshot; repeat **≥3 passes** and add
  a **bootstrap CI** on the geometric-mean ratio (resample the per-question ratios) — a sign
  test alone understates variance (Peng et al., FCS 2018).
- **Public-set contamination.** The published question set may be memorized, inflating
  correctness near ceiling. Treat correctness as *orientation*; use a **held-out private
  set** for any shipping claim.

## Honesty guardrails

- Report the metric the visual shows: a bar of pooled totals must quote the **pooled**
  ratio; the per-question geometric mean is a separate, labeled figure.
- Disclose the fairness rule: if any arm used a non-lean path (whole-file/tree dump where a
  targeted read answers), flag it — it inflates that arm's chars and biases the ratio.
