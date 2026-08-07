# How to aggregate & test benchmark results (don't let one question decide)

Load when tabulating a matchup or writing the summary. This fixes the failure the
`octocode-vs-gh-rtk-200641` run exposed: the headline "5.94× fewer characters" was
produced by **summing raw characters across questions**, and that sum was dominated by a
single question. Summing normalized/heavy-tailed per-question costs weights each question
by its absolute size, so one outlier sets the verdict.

## The unit of analysis is the QUESTION, and the arms are PAIRED

Each matchup is a pairing — Octocode (anchor) vs one baseline (gh+RTK or gh+Headroom);
the rollup presents them side by side as **two pairings** — Octocode-vs-RTK and
Octocode-vs-Headroom. Within each pairing both arms answer
the *same* question, so every metric is a matched pair (e.g. `(A_q, B_q)`). Analyse the
**per-question paired differences/ratios**, never two independently-pooled piles. Two quantities behave differently and need different aggregation:

| Quantity | Shape | Aggregate with | Never |
|---|---|---|---|
| correctness (0–10), depth/workflow (1–5) | bounded, near-ceiling | paired win/tie/loss + sign test; mean as secondary | over-reading a 0.05 mean gap |
| characters delivered | unbounded, heavy right-tailed, read as a ratio | **geometric mean of per-question ratios** + median (+ min/max, IQR) | arithmetic mean or **sum** of ratios/chars as the headline |

**Rule (Fleming & Wallace 1986, "How not to lie with statistics"; SPEC practice):** do
not use the arithmetic mean — or the pooled sum — to summarize normalized ratios. The
geometric mean is the correct central tendency for ratios; the median is the robust
backstop.

## Characters: report a distribution, not one number

For each question compute the ratio `r_q = A_q / B_q` (>1 = B leaner). Report:

1. **Geometric-mean ratio** — the honest "typical" factor.
2. **Median ratio** + **min/max** and **IQR** — the spread.
3. **Leaner win-rate** — questions where B < A, out of N, with a **sign test** p-value.
4. **Pooled sum ratio** — allowed, but label it *"aggregate context budget
   (outlier-sensitive)"* and ALWAYS pair it with two robustness disclosures:
   - **top-contributor share**: the single biggest question's % of the heavier arm's total;
   - **leave-one-out**: recompute the sum ratio with that question dropped.
   If the verdict or the factor moves materially under leave-one-out, the geometric/median
   figure is the headline and the sum is a footnote.

### Worked example — the `200641` run (why the fix matters)

| Statistic | Value | Reading |
|---|---:|---|
| pooled sum ratio A/B | **5.94×** | outlier-inflated headline |
| top contributor (Q16) share of A total | **29.6%** | one question = ~⅓ of the total |
| top-5 share of A total | **72.4%** | 5 questions set the pooled number |
| sum ratio, Q16 dropped (leave-one-out) | **4.31×** | headline drops 27% from removing 1 of 20 |
| **geometric-mean ratio** | **3.11×** | honest typical factor |
| **median ratio** | **3.99×** | robust center |
| per-question ratio range | 0.49× … 57.90× | B is *heavier* on 6 of 20 questions |
| leaner win-rate | **14/20**, sign-test **p ≈ 0.12** | directional, **not** significant in one pass |

So the defensible claim is: *"Octocode is typically ~3× leaner (geometric mean 3.1×,
median 4.0×) and was leaner on 14/20 questions (sign test p≈0.12, not significant at
α=0.05 from a single pass)."* Not "5.94× fewer characters."

## Correctness: paired, and mind the ceiling

Correctness clusters at 9–10, so mean differences (e.g. 9.65 vs 9.70) are near-noise.
Report instead:

- **paired win/tie/loss** on correctness and a **sign test** (or Wilcoxon signed-rank on
  the per-question deltas);
- **count of questions where each arm is strictly more correct**;
- keep the **correctness-first gate**: an arm that is net strictly less correct cannot win
  on footprint, whatever the char stats say.

## One pass is a point estimate — variance needs passes

A single pass gives one `r_q` per question with no within-question variance, so almost
nothing reaches significance at N=20 (see p≈0.12 above). For a **stable** claim run
**≥3 passes**, then:

- report the **per-pass geometric-mean ratio**, and the **across-pass mean ± spread**;
- run the paired test with the **question as the pairing unit**, pooled across passes
  (or average each question's ratio across passes first, then aggregate);
- flag any question whose ratio swings sign across passes — that is a stability problem,
  not a winner.

## Revised decision rule

1. **Correctness gate (paired).** If one arm is net strictly more correct (sign test on
   per-question deltas), it wins regardless of characters.
2. **If correctness is statistically indistinguishable** (near-ceiling / non-significant):
   efficiency decides via the **geometric-mean char ratio + median + leaner win-rate**,
   with the outlier disclosures above. The pooled sum alone never decides.
3. **Report uncertainty honestly.** State N, the test, and the p-value/CI. Single-pass =
   snapshot; call it a snapshot.
