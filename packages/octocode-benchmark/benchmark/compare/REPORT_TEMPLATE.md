# Report template — every scored run under `output/<run-name>/`

Canonical structure for top-notch, trendable benchmark results. A run that
deviates from this template is not comparable to prior runs and must say why.
Machine-readable twin: `kpi.json` per [`../schemas/kpi.schema.json`](../schemas/kpi.schema.json).

## Files

```
output/<run-name>/
├── SUMMARY.md            # Goal / KPI / Loop level / Checks run / Verdict (loop-report.mjs score=1)
├── kpi.json              # machine rollup — dashboards + future runs diff against this
├── <suite>.md            # one report per suite (sections below)
└── logs/<suite>/<arm>/   # raw per-question command logs {id, cmd|tool, exit, ms, bytes, tokens}
```

## SUMMARY.md — headline first

1. **Verdict line per suite** (one sentence, pre-registered decision rule
   applied): e.g. `octocode-vs-gh: WIN — 1.00 vs 0.67 uncontaminated correctness at 0.098× bytes, no guardrail regressed.`
2. **Headline table** — the whole run at a glance:

| Suite | Verdict | Correctness B vs A (uncontaminated) | Quality B vs A | Tokens B/A | Bytes B/A | Calls B/A | False-conf Δ | Contaminated Qs |
|---|---|---|---|---|---|---|---|---|

3. **Provenance block**: date, `subjectSha`, model + step budget, baseline
   versions (`gh`/`rtk`/`ast-grep`), oracle verification date, solvers per arm
   (k), and whether pass^k was met.

## Per-suite report — four required sections

### 1. Tokens usage (the cost KPI)

Per-question table, one row per question, columns per arm:

| Q | A bytes | A calls | A turns | B bytes | B calls | B turns | B/A bytes | notes |

Plus the per-arm authoritative block: runner-reported `agentTokens`
(input / output / cache_read / cache_write when available), wall-clock,
total tool uses. For `octocode-mcp-vs-cli` add the L0 row: surface preamble
bytes (MCP injected schemas vs CLI on-demand `--scheme`), cold vs warm.
State the estimator used wherever runner tokens are missing (chars/4).

### 2. Questions (the correctness KPI)

| Q | Topic | Contaminated (C≥1.0)? | A correctness | B correctness | toolUsed (B) |

- Correctness only 1.0 / 0.5 / 0 per the frozen rubric.
- Contaminated questions are shown, flagged, and **excluded from the primary
  mean** — never silently dropped.
- `toolUsed` = trajectory layer: did B exercise the question's
  `capabilityPoint`? "Right answer without the tool" is reported as a finding.

### 3. Quality of response (judge, blind)

| Q | A quality (1–5) | B quality (1–5) | one-line judge note per arm |

Judge scores exactness, concision, anchoring (`file:line` / sha cited). Judge
is blind to arm identity and re-fetches every cited sha/PR/issue; fabricated
cite = correctness 0 + `falseConfidence: true`.

### 4. Guardrails & validity (the honesty section)

- False-confidence count per arm (must not increase vs prior run).
- Dropped / timed-out questions, explicitly listed.
- Control-arm scores per question (basis of contamination flags).
- Oracle drift found during judge re-verification (what moved, what was
  corrected — corrections go into ground-truth `verification` blocks, never
  into mid-run question edits).
- For parity suites: per-call data-parity result (any divergence = surface bug,
  reported regardless of scores).

## Presentation rules (best practice)

- **Ratios over absolutes** for cost (`B/A bytes`, `B/A tokens`, `B/A calls`);
  absolutes live in the tables, ratios in the verdicts.
- **Report medians alongside means** for bytes/tokens (one 500KB dump skews a
  mean); pass^k alongside pass@1 for correctness.
- **No verdict from a draft suite** — oracle verification status is printed
  next to every verdict.
- **Losses and ties are reported as prominently as wins** (e.g. "gh-rtk:
  correctness TIE, win is 0.24× bytes"). A benchmark that only ever reports
  wins is marketing, not measurement.
- Every number in SUMMARY.md must be reproducible from `kpi.json` + `logs/`.
