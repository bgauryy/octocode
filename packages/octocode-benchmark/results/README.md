# Results

Start with [SUMMARY.md](SUMMARY.md). It uses the latest complete campaign for
each matchup and does not combine invalid or incomplete runs.

## Latest campaigns — full 30 Q × 3 passes per matchup (local build v18.1.1, gpt-5.5 judge)

Three full pairwise matchups over the complete 30-question v2 set, 3 passes each, local build
`out/octocode.js` v18.1.1; blind neutral **gpt-5.5** judge, X/Y randomized per question,
leanest-path enforced, 95% bootstrap CIs.

- **[Octocode vs gh+RTK](full-octocode-vs-rtk-162848-2026-08-07.md)** — correctness parity
  (9.29 / 9.42), Octocode **reliably ~3.2× leaner** (per-Q geo-mean 3.21×, 95% CI 2.36–4.46,
  median 2.83×, leaner 67/89, wins 56/33). Supersedes the earlier 2-pass RTK "parity"
  snapshot — the larger 3-pass run resolves toward Octocode leaner (CI now well above 1×).
- **[Octocode vs gh+Headroom](full-octocode-vs-headroom-134213-2026-08-07.md)** — Octocode
  more correct (9.30 / 8.62) **and** ~2.6× leaner (per-Q geo-mean 2.62×, 95% CI 1.87–3.71,
  median 2.77×, leaner 63/88, wins 60/28).

- **[Octocode vs plain gh](full-octocode-vs-gh-152630-2026-08-07.md)** — correctness parity
  (9.19 / 9.27), Octocode **reliably ~2.0× leaner** (per-Q geo-mean 1.99×, 95% CI 1.52–2.61,
  median 2.16×, leaner 67/89, wins 51/38). Plain `gh` is the leanest baseline, so the margin is
  smaller but still significant.

**Net:** at near-ceiling correctness parity, Octocode delivers answers in ~2.0× fewer characters
than plain `gh`, ~2.6× fewer than gh+Headroom, and ~3.2× fewer than gh+RTK (all CIs above 1×) —
a leaner context window and better model attention, with the margin widening on deep, large-file,
multi-hop research.

### Published-CLI validation (npx octocode@18.2.2)

- **[Octocode vs gh+RTK — published CLI](full-octocode-vs-rtk-011533-2026-08-08.md)** — first full
  30×3 run against the **published** `npx octocode@18.2.2` (not the local build). Correctness parity
  (8.78 / 8.96, sign p=0.40), Octocode **~2.4× leaner** (per-Q geo-mean 2.37×, 95% CI 1.70–3.35×,
  median 2.26×, leaner 66/90, p<0.0001; judge wins 47/43). Confirms the shipped artifact tracks the
  local-build result (~3.2×); pooled 11.09× is outlier-driven (Q28=24.9%, leave-one-out 8.73×).
- **[Octocode vs gh+Headroom — published CLI](full-octocode-vs-headroom-091618-2026-08-08.md)** — full
  30×3 run against **published** `npx octocode@18.2.2` vs compressed gh+Headroom 0.34.0. Correctness
  parity (8.68 / 8.68, sign p=0.59), Octocode **~2.3× leaner** (per-Q geo-mean 2.27×, 95% CI
  1.70–3.01×, median 2.60×, leaner 67/90, p<0.0001; judge wins 47/43). Compression narrows but does
  not close the gap; pooled 5.77× outlier-driven (Q28=20.3%, leave-one-out 4.98×).

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
`<COMPARISON_NAME>-<TIME>-<DATE>.md` using
[REPORT_TEMPLATE.md](../skills/octocode-benchmark/references/REPORT_TEMPLATE.md). Measurement is in Unicode
characters delivered to the runner; correctness takes precedence over
footprint. Failed probes inside a complete run remain counted.

## Supplementary probes (not headline campaigns)

| Comparison | Report | Outcome |
|---|---|---|
| Live 3-arm run (`octocode@18.2.2` vs `gh`+RTK and `gh`+Headroom) | [live-3arm-130222](live-3arm-130222-2026-08-09.md) | 30×1 fresh live run with three isolated runner agents and two blind judges. All 90 logs strict-valid. Octocode was leaner than RTK on 19/30 (geo 1.58×) and leaner than Headroom on 20/30 (geo 1.56×). Single pass only — not a headline replacement. |
| Octocode (local build, prompt+description fixes) vs `gh` + RTK | [localfix-020431](octocode-vs-gh-rtk-localfix-020431-2026-08-06.md) | 25×1, arm B = local out/octocode.js. Octocode more correct this pass (mean 9.72/9.24; paired B 5 win/17 tie/3 loss, p≈0.73 n.s.), ~3.2× leaner (geo-mean 3.24×), leaner 20/25. Targeted fixes improved Q21 (verbatim node value) and Q25 (diff-over-comment). Local-build probe, not a published-CLI headline; Q25 contested. |
