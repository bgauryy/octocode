# Full campaign — Octocode (published npx CLI **v18.2.2**) vs gh + RTK

**Outcome:** At near-ceiling correctness parity, the **published** Octocode CLI
(`npx octocode@18.2.2`) is **reliably ~2.4× leaner** than gh+RTK across the full 30-question v2
GitHub set over 3 passes. Per-question geometric-mean char ratio **2.37×** (95% bootstrap CI
**1.70–3.35×**, entirely above 1×), median 2.26×, Octocode leaner on **66/90** instances
(sign test p<0.0001). Correctness was a statistical tie (mean 8.78 vs 8.96/10, sign test
p=0.40) — rtk edged raw correctness by 0.18, never at a lower character cost. Judge winners
split **47 (octocode) / 43 (rtk)** because on the ~⅓ of questions where a single `gh` call or
snippet already answers, rtk is leaner and takes the char tiebreak.

## Run metadata

- **Question set:** `compare/github-questions/` v2, 30 questions.
- **Matchup:** Octocode (anchor) vs gh + RTK (baseline). Pairwise; this campaign ran RTK only.
- **Octocode arm:** **published npx `octocode@18.2.2`** via new pinned wrapper `compare/bin/octoc1822`
  (`npx -y octocode@18.2.2 tools …`). This is the first full campaign against the *published* CLI
  rather than the local monorepo build.
- **Baseline:** `gh` 2.96.0 + `rtk` 0.44.2 (`compare/bin/rtkm`, `rtk gh <args>`).
- **Runner model:** claude-sonnet-4-6 (anthropic). **Judge model:** gpt-5.5 (guy-provider-openai) —
  neutral, different family from the runner; blind X/Y packet, seed=42, tool identity redacted.
- **Passes:** 3 (isolated runner per arm×batch×pass; batched Q1-15/Q16-30 per arm).
- **Measurement:** `total_chars = model-in + model-out` from instrumented per-question JSONL,
  strictly validated byte-faithfully (see caveats).
- **Primer commit:** `RUNNER_TOOL_CONTEXT.md` as of this campaign date.

## Summary (paired, per question)

| Metric | vs RTK |
|---|---:|
| Correctness — win / tie / loss (octocode) | **22 / 39 / 29** (sign p=0.40) |
| Mean correctness — octocode / rtk | 8.78 / 8.96 (of 10) |
| Judge winners — octocode / rtk / unresolved | **47 / 43 / 0** |
| Research depth — mean (octocode / rtk, ceiling) | ~4.3 / ~4.4 |
| Workflow — mean (octocode / rtk) | ~3.9 / ~3.5 |
| **Char ratio — geometric mean (headline)** | **2.37× leaner** |
| Char ratio — 95% bootstrap CI | **1.70 … 3.35×** |
| Char ratio — median (min … max) | 2.26× (0.04 … 111.20×) |
| Questions Octocode leaner (of 90) + sign p | **66 / 90**, p<0.0001 |
| Chars pooled sum — *outlier-sensitive* | 11.09× (rtk 24.94M / octocode 2.25M) |
| — top contributor (Q28/p2) share of rtk total | 24.9% |
| — pooled ratio, leave-one-out (drop top) | 8.73× |
| Per-pass geo-mean (P1 / P2 / P3) | 2.69× / 2.03× / 2.45× |

## Best-on (roll-up)

- **Accuracy:** parity (8.78 vs 8.96, p=0.40). Both reach the same core facts on most questions;
  rtk's tiny edge came from a few verbatim-detail captures (e.g. Q22 range totals, Q24 call sites).
- **Workflow / characters:** Octocode — leaner on 66/90, geo-mean 2.37× (CI 1.70–3.35×). rtk relays
  raw `gh` output uncompressed, so tree/large-file/multi-hop questions cost far more: **Q28 56×,
  Q22 34×, Q17 18×, Q23 9.5×, Q29 9.3×, Q21 8×, Q16 7.5×, Q26 7×, Q12 7×**.
- **Where rtk wins chars:** questions a single snippet/call answers — **Q2 0.05×, Q13 0.22×,
  Q8 0.30×, Q15 0.33×, Q14 0.51×, Q25 0.72×, Q10 0.71×** — rtk is leaner and takes those.
- **Overall:** at correctness parity, the published Octocode CLI wins the matchup on leanness;
  the margin widens on deep/large-file/multi-hop research and inverts only on single-hit lookups.

## Fairness & measurement caveats

- **Outlier disclosure:** the pooled 11.09× is outlier-sensitive — Q28/p2 alone is 24.9% of rtk's
  total (rtk averaged ~4.24M chars on Q28, a `NousResearch/hermes-agent` memory question where the
  rtk path fetched large trees/files). Headline uses the geo-mean (2.37×) + CI, not the pooled sum.
- **Stability:** the leaner side flips across passes on Q1, Q3, Q7, Q10, Q11, Q14, Q18, Q25, Q30 —
  these are near-1× questions where either arm can be marginally leaner; not decisive winners.
- **Byte-faithful validation:** the shared `sumlog.py` re-reads artifacts with universal-newline
  translation, which strips `\r` and produced a false CRLF char/hash mismatch on `rtk-p1-Q25`
  (recorded 6681 = re-read 6679 + 2 CR). Validation was done byte-faithfully (`build_metrics.py`,
  decode without newline translation) — recorded capture-time counts reconcile exactly (len+sha
  match). No data loss; both arms measured identically. Upstream `sumlog.py` has this CRLF bug.
- **Correctness ground truth** was established independently by the blind gpt-5.5 judge; a handful
  of runner Q13 answers cited differing issue/PR numbers across passes (a public-repo drift /
  ambiguity), resolved per-instance by the judge — no question was left unresolved.

## Artifacts

- Campaign: `campaigns/full-rtk-011533-2026-08-08/`
- Logs: `{octocode,rtk}-p{1,2,3}-Q{1..30}.jsonl` (+ `-artifacts/`)
- Answers: `answers/{arm}-p{pass}.md`
- Blind packet + map: `blind-packet.md` (+ `.MAP.secret.txt`, seed=42)
- Verdicts (gpt-5.5): `judge/p{1,2,3}-Q{n}.md`; scores `judge/scores-p{pass}-h{1,2}.csv`
- Metrics: `metrics.json` (validated) · aggregate `metrics-agg.json` · `aggregate.py`

## Per-question table (mean across 3 passes; Chars = total = model-in + model-out)

| Q | Corr O/B | Depth O/B | Workflow O/B | Chars O / B (mean) | Winner (3p) | Ratio B/O |
|---|---|---|---|---|---|---|
| Q1 | 10.0/10.0 | 4.0/4.3 | 4.0/4.0 | 17,086 / 28,434 | O (2-1) | 1.54× |
| Q2 | 10.0/10.0 | 4.0/4.3 | 3.0/5.0 | 13,029 / 630 | B (0-3) | 0.05× |
| Q3 | 9.3/10.0 | 4.3/5.0 | 3.0/3.7 | 13,229 / 29,061 | O (2-1) | 2.05× |
| Q4 | 9.3/9.7 | 4.7/5.0 | 3.3/1.7 | 40,403 / 215,453 | O (2-1) | 5.11× |
| Q5 | 9.7/8.0 | 5.0/3.7 | 4.0/3.3 | 48,479 / 75,455 | O (3-0) | 1.45× |
| Q6 | 9.0/9.3 | 4.3/4.3 | 3.0/3.0 | 24,408 / 41,432 | O (2-1) | 1.78× |
| Q7 | 10.0/10.0 | 4.7/4.7 | 4.7/5.0 | 8,266 / 8,643 | B (1-2) | 1.09× |
| Q8 | 9.0/9.0 | 4.7/3.7 | 3.7/4.3 | 26,188 / 9,720 | B (1-2) | 0.30× |
| Q9 | 9.0/10.0 | 4.3/4.7 | 4.0/2.3 | 23,285 / 95,874 | B (1-2) | 3.33× |
| Q10 | 5.0/7.0 | 2.7/3.3 | 2.7/4.0 | 13,990 / 9,011 | B (1-2) | 0.71× |
| Q11 | 9.3/9.3 | 4.7/4.7 | 3.7/2.0 | 37,599 / 123,203 | O (2-1) | 2.83× |
| Q12 | 10.0/10.0 | 5.0/5.0 | 4.3/2.0 | 15,601 / 112,288 | O (3-0) | 6.98× |
| Q13 | 9.3/7.3 | 4.7/4.3 | 3.3/4.7 | 41,827 / 7,459 | B (1-2) | 0.22× |
| Q14 | 10.0/9.3 | 4.7/4.3 | 4.0/4.3 | 19,454 / 11,912 | B (1-2) | 0.51× |
| Q15 | 10.0/10.0 | 5.0/4.7 | 3.7/5.0 | 24,194 / 10,965 | B (0-3) | 0.33× |
| Q16 | 10.0/9.3 | 4.3/4.3 | 4.7/3.3 | 9,589 / 231,291 | O (3-0) | 7.50× |
| Q17 | 7.0/7.3 | 3.7/3.7 | 4.3/2.0 | 13,304 / 233,883 | O (2-1) | 18.17× |
| Q18 | 9.3/10.0 | 4.7/4.7 | 4.7/5.0 | 7,894 / 8,322 | O (2-1) | 1.04× |
| Q19 | 9.3/9.3 | 4.7/4.7 | 4.3/3.0 | 21,091 / 108,194 | O (2-1) | 6.30× |
| Q20 | 8.7/9.3 | 4.3/4.7 | 4.3/3.7 | 12,906 / 40,792 | B (1-2) | 3.09× |
| Q21 | 7.7/9.0 | 4.3/4.7 | 3.3/2.7 | 71,208 / 562,588 | B (1-2) | 7.96× |
| Q22 | 8.3/10.0 | 3.7/4.7 | 3.0/2.3 | 29,014 / 994,113 | B (1-2) | 34.28× |
| Q23 | 7.7/7.7 | 4.0/4.0 | 4.3/3.0 | 12,219 / 122,439 | B (1-2) | 9.49× |
| Q24 | 9.3/10.0 | 4.3/4.7 | 4.3/3.7 | 21,278 / 71,379 | B (1-2) | 3.65× |
| Q25 | 7.3/7.7 | 4.0/4.0 | 4.3/4.0 | 25,221 / 19,537 | B (1-2) | 0.72× |
| Q26 | 6.7/6.3 | 3.7/3.3 | 4.3/3.7 | 5,842 / 46,699 | O (3-0) | 7.02× |
| Q27 | 8.7/8.7 | 4.3/4.3 | 5.0/4.3 | 8,639 / 19,162 | O (2-1) | 1.91× |
| Q28 | 6.0/7.3 | 3.3/4.0 | 3.0/1.3 | 65,775 / 4,239,152 | B (1-2) | 56.34× |
| Q29 | 8.3/9.3 | 4.0/4.7 | 3.7/2.3 | 54,201 / 806,337 | B (1-2) | 9.30× |
| Q30 | 10.0/8.3 | 5.0/4.0 | 4.3/4.3 | 24,689 / 30,810 | O (3-0) | 1.09× |

O = Octocode (`npx octocode@18.2.2`); B = gh+RTK. Winner (3p) = per-pass judge winners; Ratio B/O is
the per-question geometric-mean char ratio (>1 = Octocode leaner). Full per-instance verdicts and
scores in `campaigns/full-rtk-011533-2026-08-08/judge/`.

## Bottom line

Equally correct (8.78 vs 8.96, not significant). At that parity the **published Octocode CLI
v18.2.2 is typically ~2.4× leaner** than gh+RTK (geo-mean 2.37×, 95% CI 1.70–3.35×, median 2.26×),
leaner on 66/90 instances — and the advantage is shape-dependent: large on deep, large-file, and
multi-hop questions (Q28, Q22, Q17, Q23, Q29, Q21, Q16, Q12), inverted on single-hit lookups where
one `gh` call answers (Q2, Q13, Q8, Q15). This published-CLI result is directionally consistent with
the prior local-build run (which measured ~3.2×); the published CLI's typical factor here is ~2.4×.
