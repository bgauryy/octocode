# Results

Start with [SUMMARY.md](SUMMARY.md). It uses the latest complete campaign for
each matchup and does not combine invalid or incomplete runs.

## Current headline reports

| Comparison | Report | Outcome |
|---|---|---|
| Octocode vs `gh` | [233502](octocode-vs-gh-233502-2026-08-05.md) | Equal correctness; Octocode 1.88× smaller. |
| Octocode vs `gh` + RTK | [081914](octocode-vs-gh-rtk-081914-2026-08-05.md) | RTK wins on correctness; Octocode uses 10.6% fewer characters. |
| Octocode vs `gh` + Headroom | [115145](octocode-vs-gh-headroom-115145-2026-08-05.md) | Octocode wins 29–22 and uses 62.2% fewer characters. |

Historical complete reports may remain in this directory for traceability but
are not silently pooled across different harness generations. Invalid and
incomplete reports are removed rather than included with caveats in the
headline calculation.

Write new reports as
`<comparison-name>-<HHMMSS>-<YYYY-MM-DD>.md` using
[REPORT_TEMPLATE.md](../REPORT_TEMPLATE.md). Measurement is in Unicode
characters delivered to the runner; correctness takes precedence over
footprint. Failed probes inside a complete run remain counted.
