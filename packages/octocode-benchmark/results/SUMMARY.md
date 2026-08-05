# Benchmark results summary

This page reports the latest complete result for each GitHub CLI matchup.
Invalid or incomplete campaigns are not included in headline calculations.
Historical complete reports remain available for traceability, but results with
different harness generations are not summed into one synthetic total.

## Latest complete campaigns

| Matchup | Questions / repeats | Correctness (baseline / Octocode) | Characters (baseline / Octocode) | Verdict |
|---|---:|---:|---:|---|
| [`gh` + RTK](octocode-vs-gh-rtk-200641-2026-08-05.md) | 20 × 1 | 9.65 / **9.70** (n.s.; B higher only on Q5) | geo-mean ratio **3.11×**, median 3.99× (sum 2,145,545 / 361,409 = 5.94×, Q16 = 29.6%) | Correctness indistinguishable; Octocode typically ~3× leaner, leaner on 14/20 (sign test p≈0.12, n.s. single pass); pooled 5.94× is outlier-inflated (leave-one-out 4.31×). Prior [20×2 v2](octocode-vs-gh-rtk-191906-2026-08-05.md) kept for multi-pass evidence. |
| [`gh` + Headroom](octocode-vs-gh-headroom-1845-2026-08-05.md) | 20 × 3 | 7.50 / **9.10** | 1,950,658 / **518,758** | Headline (multi-pass, Headroom 0.34.0): Octocode clearly more correct, preferred 17/3, 3.76× cheaper. Corroborating single-pass 20×1 snapshot: [200034](octocode-vs-gh-headroom-200034-2026-08-05.md) — Octocode more correct (9.70 / 8.35), 6.3× smaller. |

## What the current evidence supports

- **Octocode is consistently smaller on aggregate** in these three latest
  complete campaigns.
- **Correctness ranges from tie to a clear Octocode win, depending on the
  baseline's failure mode.** RTK was statistically indistinguishable (near
  ceiling); against Headroom Octocode was **clearly more correct** (182 vs 150 of
  200) because the compressed `gh` arm made 5 confident errors on deterministic
  package.json/metadata questions. Correctness-first scoring means a leaner
  answer never overrides a wrong one.
- **The strongest current evidence is the Headroom campaign.** It has three
  passes per arm, strict artifact-backed measurement, complete log/answer
  census, blind grading, and an independent measurement audit.
- These campaigns reuse one small public question suite (20 questions for the
  latest RTK and Headroom headline reports). They are paired campaign results,
  not independent samples of all repository-research work.

## Latest Headroom detail (campaign-20260805-1845, 20 × 3, Headroom 0.34.0)

| Metric | `gh` + Headroom | Octocode |
|---|---:|---:|
| Correctness (sum /200) | 150 | **182** |
| Research depth (mean /5) | 3.6 | **3.7** |
| Workflow (mean /5) | 3.3 | **4.15** |
| Delivered characters (chars-in) | 1,950,658 | **518,758** |
| Per-question preference wins | 3 | **17** |
| Failed calls retained | **5–7** | 21 |

Headroom compressed ~2.19M raw `gh` characters to ~1.95M (10.9% reduction);
Octocode still delivered **≈3.76× fewer** context characters (519K vs 1.95M)
via targeted/minified reads, not a compressor. Arm A made **5 confident errors
on deterministic package.json/metadata questions** (Q7, Q10, Q14, Q16, Q18) —
the correctness gap. Octocode's honest weaknesses: Q3 commit-content retrieval
(33 calls / 9 failed; A won with a targeted 3-call path) and a higher raw
failed-call count from schema/empty-search retries; A also won Q6 and Q13.

## Validity policy

A published headline campaign must contain every planned question and pass,
preserve every research call—including failed probes—and pass its applicable
measurement checks. Failed calls inside a complete campaign remain counted as
workflow waste; deleting them would bias the result. A campaign that loses
measurements, omits answers, mixes units, or cannot classify its transport is
excluded instead of repaired after the fact.

The incomplete 15/17 Headroom campaign and the Headroom campaign explicitly
marked invalid for a winner claim were removed from `results/` and do not
contribute to this summary.

## Scope and interpretation

The benchmark measures answer correctness, evidence depth, workflow quality,
and Unicode characters delivered by each CLI. It does not directly measure
tokens, model latency, monetary cost, or product-wide capability. Use public
benchmark results as orientation; use private held-out failures for shipping
decisions.

See [benchmark design](../BENCHMARK.md), [scoring](../SCORING.md),
[judging](../JUDGING.md), and the [run instructions](../INSTRUCTIONS.md).
