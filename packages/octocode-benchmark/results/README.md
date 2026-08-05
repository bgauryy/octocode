# Results

Start with [SUMMARY.md](SUMMARY.md). It uses the latest complete campaign for
each matchup and does not combine invalid or incomplete runs.

## Current headline reports

| Comparison | Report | Outcome |
|---|---|---|
| Octocode vs `gh` | [233502](octocode-vs-gh-233502-2026-08-05.md) | Equal correctness; Octocode 1.88× smaller. |
| Octocode vs `gh` + RTK | [200641](octocode-vs-gh-rtk-200641-2026-08-05.md) | 20 questions × 1: correctness indistinguishable (B higher only on Q5, near-ceiling). Octocode typically ~3× leaner (geo-mean 3.11×, median 3.99×), leaner on 14/20 (sign test p≈0.12, n.s.); pooled-sum 5.94× is outlier-inflated (Q16 = 29.6%; leave-one-out 4.31×). Single-pass snapshot; earlier [20×2 v2](octocode-vs-gh-rtk-191906-2026-08-05.md) kept for multi-pass evidence. |
| Octocode vs `gh` + Headroom | [1845](octocode-vs-gh-headroom-1845-2026-08-05.md) | 20×3, Headroom 0.34.0: Octocode clearly more correct (7.50/9.10), preferred 17/3, 3.76× cheaper. Corroborating single-pass [200034](octocode-vs-gh-headroom-200034-2026-08-05.md): Octocode more correct (9.70/8.35), 6.3× smaller. |

Historical complete reports may remain in this directory for traceability but
are not silently pooled across different harness generations. Invalid and
incomplete reports are removed rather than included with caveats in the
headline calculation.

Write new reports as
`<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` using
[REPORT_TEMPLATE.md](../REPORT_TEMPLATE.md). Measurement is in Unicode
characters delivered to the runner; correctness takes precedence over
footprint. Failed probes inside a complete run remain counted.
