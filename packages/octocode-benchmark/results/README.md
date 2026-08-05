# Results

Start with [SUMMARY.md](SUMMARY.md). It uses the latest complete campaign for
each matchup and does not combine invalid or incomplete runs.

## Current headline reports

| Comparison | Report | Outcome |
|---|---|---|
| Octocode vs `gh` | [233502](octocode-vs-gh-233502-2026-08-05.md) | Equal correctness; Octocode 1.88× smaller. |
| Octocode vs `gh` + RTK | [011611](octocode-vs-gh-rtk-011611-2026-08-06.md) | **25 questions × 1 (v2, incl. advanced Q21–Q25)**: correctness indistinguishable (means A 9.24 / B 9.18; paired B 3 win / 8 loss / 14 tie; sign test p≈0.23, n.s., near ceiling). Octocode ~5× leaner (geo-mean 4.98×, median 6.65×), leaner on 21/25; pooled-sum 11.54× outlier-sensitive (Q22 = 24.8%; leave-one-out 9.40×). Single-pass. Prior [20-question 200641](octocode-vs-gh-rtk-200641-2026-08-05.md) kept for history. |
| Octocode vs `gh` + Headroom | [011859](octocode-vs-gh-headroom-011859-2026-08-06.md) | **25 questions × 1 (v2, incl. advanced Q21–Q25)**, Headroom 0.34.0: correctness indistinguishable near ceiling (means A 7.84 / B 8.96; paired B 9 win / 3 loss / 13 tie; sign test p≈0.15). Octocode **~5.4× leaner** (geo-mean 5.38×, median 6.09×), leaner 22/25 (p≈0.00016); pooled 8.54× outlier-sensitive (Q13 = 15.8%; leave-one-out 5.0–6.6×). Compression lost exact structured-fact fidelity (Q14/Q18). Single-pass. Stronger multi-pass evidence kept: [20×3 1845](octocode-vs-gh-headroom-1845-2026-08-05.md) (7.50/9.10, preferred 17/3, 3.76×). |

Historical complete reports may remain in this directory for traceability but
are not silently pooled across different harness generations. Invalid and
incomplete reports are removed rather than included with caveats in the
headline calculation.

Write new reports as
`<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` using
[REPORT_TEMPLATE.md](../REPORT_TEMPLATE.md). Measurement is in Unicode
characters delivered to the runner; correctness takes precedence over
footprint. Failed probes inside a complete run remain counted.

## Supplementary probes (not headline campaigns)

| Comparison | Report | Outcome |
|---|---|---|
| Octocode (local build, prompt+desc fixes) vs `gh` + RTK | [localfix-020431](octocode-vs-gh-rtk-localfix-020431-2026-08-06.md) | 25×1, arm B = local out/octocode.js. Octocode more correct this pass (mean 9.72/9.24; paired B 5 win/17 tie/3 loss, p≈0.73 n.s.), ~3.2× leaner (geo-mean 3.24×), leaner 20/25. Targeted fixes improved Q21 (verbatim node value) & Q25 (diff-over-comment). Local-build probe, not a published-CLI headline; Q25 contested. |
