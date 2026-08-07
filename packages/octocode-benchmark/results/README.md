# Results

Start with [SUMMARY.md](SUMMARY.md). It uses the latest complete campaign for
each matchup and does not combine invalid or incomplete runs.

## Latest campaigns — full 30 Q × 3 passes per matchup (local build v18.1.1, gpt-5.5 judge)

Two full pairwise matchups over the complete 30-question v2 set, 3 passes each, local build
`out/octocode.js` v18.1.1; blind neutral **gpt-5.5** judge, X/Y randomized per question,
leanest-path enforced, 95% bootstrap CIs.

- **[Octocode vs gh+RTK](full-octocode-vs-rtk-162848-2026-08-07.md)** — correctness parity
  (9.29 / 9.42), Octocode **reliably ~3.2× leaner** (per-Q geo-mean 3.21×, 95% CI 2.36–4.46,
  median 2.83×, leaner 67/89, wins 56/33). Supersedes the earlier 2-pass RTK "parity"
  snapshot — the larger 3-pass run resolves toward Octocode leaner (CI now well above 1×).
- **[Octocode vs gh+Headroom](full-octocode-vs-headroom-134213-2026-08-07.md)** — Octocode
  more correct (9.30 / 8.62) **and** ~2.6× leaner (per-Q geo-mean 2.62×, 95% CI 1.87–3.71,
  median 2.77×, leaner 63/88, wins 60/28).

**Net:** at near-ceiling correctness parity, Octocode delivers answers in ~2.6–3.2× fewer
characters than *both* baselines (both CIs above 1×) — a leaner context window and better model
attention, with the margin widening on deep, large-file, multi-hop research.

### Superseded (earlier, smaller runs)

- [campaign-2pass-183432](campaign-2pass-183432-2026-08-06.md) (25×2, published CLI) — found
  vs-Headroom 2.87× (stable) but vs-RTK parity (CI 0.70–2.05 straddled 1×). The RTK result is
  now resolved by the 30×3 run above. Per-pass: [P1 175653](campaign-175653-2026-08-06.md).

## Historical reports (restored, prior methodology)

Kept for traceability; superseded by the 2-pass campaign above and not summed into the
headline (different harness generations / single-pass or v1 suite):

| Report | Note |
|---|---|
| [gh+RTK 011611](octocode-vs-gh-rtk-011611-2026-08-06.md) | 25×1 v2, published CLI; RTK char win overstated by single pass |
| [gh+RTK 200641](octocode-vs-gh-rtk-200641-2026-08-05.md) · [183927](octocode-vs-gh-rtk-183927-2026-08-05.md) · [191906](octocode-vs-gh-rtk-191906-2026-08-05.md) | earlier v1/interim RTK runs |
| [gh+Headroom 011859](octocode-vs-gh-headroom-011859-2026-08-06.md) | 25×1 v2 |
| [gh+Headroom 1845](octocode-vs-gh-headroom-1845-2026-08-05.md) · [200034](octocode-vs-gh-headroom-200034-2026-08-05.md) | 20×3 and earlier Headroom runs |

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
