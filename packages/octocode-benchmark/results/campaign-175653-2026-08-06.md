# 3-arm campaign — Octocode vs gh+RTK vs gh+Headroom (25×1, 2026-08-06)

**Bottom line:** At **near-ceiling correctness parity**, Octocode delivered answers in far
fewer context characters than **both** baselines: **~2.2× leaner than gh+RTK** (geo-mean;
95% CI 1.10–3.92; median 3.46×; leaner 19/25) and **~2.5× leaner than gh+Headroom** (geo-mean;
95% CI 1.37–4.37; median 3.57×; leaner 18/25). Both CIs sit above 1× → the leanness win is
real, not an outlier artifact. This is the first campaign run under the fixed methodology
(3 isolated runners/question, **neutral gpt-5.5 judge**, **randomized X/Y/Z order**,
**leanest-legal path enforced on every arm**, bootstrap CIs).

## Setup

| Field | Value |
|---|---|
| Question set | shared `compare/github-questions/` v2, 25 questions |
| Arm A | Octocode — `npx -y octocode@18.1.1 tools …` (`bin/octoc1811`) |
| Arm B | gh 2.96.0 + RTK 0.44.2 — `rtk gh …` (`bin/rtkm`) |
| Arm C | gh 2.96.0 + Headroom 0.34.0 (onnx) — `./bin/ghc …` |
| Runners | 6 isolated agents (claude-sonnet-4-6), A/B/C × two halves; leanest-path required |
| Judge | **5 blind agents, gpt-5.5 (neutral family)**, X/Y/Z randomized per question |
| Measurement | per-question JSONL, `total_chars` = model-in + model-out; correctness/depth/workflow from judge |
| Campaign dir | `compare/octocode-vs-gh-headroom/tmp/campaign-175653-2026-08-06/` |

## Correctness / depth / workflow (blind-graded, means, n=25)

| Arm | Correctness | Depth | Workflow |
|---|---:|---:|---:|
| **Octocode** | 9.44 | 4.48 | 4.96 |
| gh+RTK | **9.68** | 4.56 | 4.96 |
| gh+Headroom | 9.20 | 4.32 | 4.96 |

Paired (correctness): Octocode vs RTK **3 win / 16 tie / 6 loss** (RTK marginally higher,
near-ceiling, n.s.); Octocode vs Headroom **5 / 16 / 4**. Correctness is a three-way tie
within noise. Octocode's losses cluster on the hardest flow/graph questions (Q19, Q21, Q23,
Q24) and one over-terse answer (Q10); Headroom's one real miss was **Q13** (answered a
different Redis BITFIELD bug → corr 2).

## Characters (paired per question; baseline ÷ Octocode, >1 = Octocode leaner)

| Pairing | geo-mean | 95% CI | median | Octocode leaner | pooled |
|---|---:|---:|---:|---:|---:|
| vs **gh+RTK** | **2.22×** | 1.10–3.92 | 3.46× | 19/25 | 4.24× (1,872,659 / 441,409) |
| vs **gh+Headroom** | **2.49×** | 1.37–4.37 | 3.57× | 18/25 | 6.03× (2,660,603 / 441,409) |

Bootstrap CI = 5,000 resamples of the 25 per-question ratios. Octocode's advantage
concentrates on large-file / multi-file / commit-scoped questions where `gh` must pull whole
files (region reads win): Q23 17×, Q17 11–24×, Q24 9–16×, Q21 11–14×, Q12 7×, Q8 10×. It is
*heavier* on a few small ones (Q2 absence-check over-fetch; Q13/Q22/Q15 where a compressor or
tiny raw read wins).

## Per-question table

| Q | Corr A/B/C | Chars A | Chars B | Chars C | B/A | C/A |
|---|---|--:|--:|--:|--:|--:|
| Q1 | 10/10/10 | 16,827 | 28,610 | 19,412 | 1.7× | 1.2× |
| Q2 | 9/8/9 | 59,307 | 264 | 2,484 | 0.0× | 0.0× |
| Q3 | 10/10/10 | 11,267 | 34,140 | 7,443 | 3.0× | 0.7× |
| Q4 | 10/10/10 | 29,595 | 202,159 | 287,689 | 6.8× | 9.7× |
| Q5 | 9/8/9 | 48,401 | 53,427 | 177,785 | 1.1× | 3.7× |
| Q6 | 10/10/10 | 9,970 | 37,808 | 45,347 | 3.8× | 4.5× |
| Q7 | 10/10/8 | 5,123 | 7,783 | 2,355 | 1.5× | 0.5× |
| Q8 | 10/10/10 | 5,764 | 58,505 | 28,205 | 10.2× | 4.9× |
| Q9 | 10/10/10 | 14,500 | 51,156 | 51,745 | 3.5× | 3.6× |
| Q10 | 8/10/9 | 3,674 | 14,496 | 3,666 | 3.9× | 1.0× |
| Q11 | 10/10/10 | 22,362 | 105,694 | 194,081 | 4.7× | 8.7× |
| Q12 | 10/10/10 | 11,427 | 81,546 | 82,856 | 7.1× | 7.3× |
| Q13 | 10/10/2 | 18,528 | 10,206 | 3,568 | 0.6× | 0.2× |
| Q14 | 10/10/10 | 7,700 | 12,873 | 15,443 | 1.7× | 2.0× |
| Q15 | 10/10/10 | 5,363 | 3,871 | 3,591 | 0.7× | 0.7× |
| Q16 | 10/10/10 | 9,424 | 29,424 | 23,062 | 3.1× | 2.4× |
| Q17 | 10/10/10 | 14,554 | 160,835 | 354,865 | 11.1× | 24.4× |
| Q18 | 9/10/10 | 6,789 | 5,230 | 7,019 | 0.8× | 1.0× |
| Q19 | 8/10/7 | 22,083 | 91,830 | 196,581 | 4.2× | 8.9× |
| Q20 | 10/10/10 | 13,766 | 47,639 | 61,955 | 3.5× | 4.5× |
| Q21 | 7/9/10 | 49,073 | 534,755 | 694,024 | 10.9× | 14.1× |
| Q22 | 10/10/10 | 19,311 | 4,718 | 16,550 | 0.2× | 0.9× |
| Q23 | 8/9/10 | 10,715 | 180,508 | 182,723 | 16.8× | 17.1× |
| Q24 | 9/10/8 | 10,876 | 102,215 | 176,901 | 9.4× | 16.3× |
| Q25 | 9/8/8 | 15,010 | 12,967 | 21,253 | 0.9× | 1.4× |

## Best-on roll-up

- **Accuracy:** three-way tie near ceiling (RTK 9.68 ≥ Octocode 9.44 ≥ Headroom 9.20; n.s.).
- **Workflow:** tie (4.96 all) — all arms kept leanest-legal paths this run (the fairness
  rule held, unlike earlier campaigns).
- **Characters:** **Octocode clearly best** — ~2.2× vs RTK, ~2.5× vs Headroom (geo-mean,
  CIs > 1), median ~3.5×, leaner on 18–19/25.
- **Depth:** RTK 4.56 ≈ Octocode 4.48 > Headroom 4.32.

## Caveats

Single pass (snapshot; repeat ≥3 for significance). Correctness is contaminated-ceiling on a
public set — treat as orientation, not a shipping gate. Judge is one neutral model (gpt-5.5);
X/Y/Z order randomized to control position bias. A few Octocode plans over-fetched (Q2) and a
few baseline reads were genuinely small (Q13/Q15/Q22) — errors cut both ways; the geo-mean
with CI is the honest headline.

## Bottom line

Correctness parity across all three tools; **Octocode answers with ~2.2–2.5× fewer characters
than both `gh`+RTK and `gh`+Headroom** (95% CIs above 1×), with the margin widening on deep
large-file/multi-hop research and narrowing-to-negative on trivial single-value lookups.
