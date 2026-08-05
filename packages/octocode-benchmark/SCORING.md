# Scoring (by hand)

Grader prose is the evidence. Score each answer, per question, on four things:

- **Correctness** — did it answer every material part? (0–10)
- **Research depth** — how well the evidence supports the answer (1–5)
- **Workflow** — how clean the path was: right calls, no wasted or redundant work (1–5)
- **Chars in/out** — characters pulled into context (raw CLI output), per question

**Aggregate per question, paired — the question is the unit and the arms are matched
pairs.** Do not pool raw outputs and compare totals: summing characters weights each
question by its absolute size, so one heavy question sets the verdict. For characters,
headline the **geometric mean of per-question A/B ratios** + **median** + **leaner
win-rate (sign test)**; report a pooled sum only with its top-contributor share and a
leave-one-out. For correctness/depth/workflow (near ceiling) use **paired win/tie/loss +
sign test**, not mean gaps. Full method + worked example: `skills/octocode-benchmark/references/aggregation-and-stats.md`.

**Decide with correctness first.** Compare Octocode (B) to baseline (A):

- B net strictly more correct (paired sign test) → B wins; net less correct → B loses
- correctness statistically indistinguishable → efficiency decides: leaner by
  **geometric-mean char ratio** (never the pooled sum alone)

A confidently-wrong answer (major false confidence) blocks a win regardless of efficiency.
One full pass is a snapshot with no within-question variance — repeat (≥3 passes) and test
with the question as the pairing unit for a stable, significant result.
