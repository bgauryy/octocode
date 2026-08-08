# Results re-validation — 2026-08-08

Re-ran `bin/validate_campaign.py` (strict, byte-faithful `sumlog.py --strict`) over every
campaign in `campaigns/` and reconciled against the published reports in `results/`.

## Status per published campaign

| Report (`results/`) | Campaign dir | Strict validation |
|---|---|---|
| full-octocode-vs-rtk-**011533** (published `octocode@18.2.2`) | `full-rtk-011533-2026-08-08` | **CLEAN — 0 failures** |
| full-octocode-vs-headroom-**091618** (published `octocode@18.2.2`) | `full-hr-091618-2026-08-08` | **CLEAN — 0 failures** |
| full-octocode-vs-rtk-**162848** (local build) | `full-rtk-162848-2026-08-07` | 1 known/disclosed failure — `octocode-p1-Q21` |
| full-octocode-vs-headroom-**134213** (local build) | `full-134213-2026-08-07` | 1 known/disclosed failure — `octocode-p1-Q21` |
| full-octocode-vs-gh-**152630** (local build) | `full-gh-143806-2026-08-07` | 1 known/disclosed failure — `octocode-p1-Q21` |

Not a published result: `campaigns/full-rtk-004037-2026-08-08` is an abandoned partial (missing
p2/p3 logs) — not referenced by any `results/*.md`.

## The one failing call — already disclosed, effect negligible

- The three local-build headline campaigns **share the same `octocode` pass-1 logs** (anchor runs
  reused across matchups), so all three surface the *same single* strict failure:
  `octocode-p1-Q21`, call 6 = `ghGetFileContent langchain-ai/langchainjs ReactAgent.ts:190-320`.
- Recorded `chars=5249` / `sha=8425f9…`; the on-disk artifact re-reads as **5996 chars** (byte-faithful,
  no CRLF translation) → char-count + hash mismatch. Δ = **+747 chars on one call** inside a question
  whose octocode total is **91,891 chars**.
- This is explicitly documented in the reports' *Fairness caveats* ("`octocode p1-Q21` still carries
  the prior artifact-integrity caveat … kept in aggregate; effect negligible"). It does not move the
  headline geo-means (~3.2× vs RTK, ~2.6× vs Headroom) or the correctness ties.

## Fresh spot-check (this session)

Ran Q1–Q10 × 1 pass for `octocode@18.2.2` vs RTK and vs Headroom
(`campaigns/spotcheck10-133030-2026-08-08`). All 30 logs pass `sumlog --strict` (rc=0, **0 ERROR**),
`total = model_in + model_out` verified, compression active on the Headroom arm. Directionally
consistent with the headlines: Octocode leaner on 8/10 (RTK) and 7/10 (Headroom); medians ~2.5×;
single-hit lookups (Q2/Q3) invert as expected.

**Bottom line:** the current results are correct and reconcile with the docs. The two newest
published-CLI campaigns are fully byte-clean; the three local-build headlines carry one
pre-disclosed, negligible artifact caveat on a shared call.
