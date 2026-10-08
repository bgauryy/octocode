# Full campaign — Octocode (local build) vs gh + Headroom

**Outcome:** Octocode is both **more correct** and **markedly leaner** than gh+Headroom across
the full 30-question v2 GitHub set, over 3 passes. Octocode won 60 of 88 scored
question-instances (headroom 28), at higher mean correctness (9.30 vs 8.62/10) while
using **~2.6× fewer characters through the model** (geometric mean of the per-question ratio).

## Run metadata

- **Question set:** `compare/github-questions/` v2, 30 questions.
- **Matchup:** Octocode (anchor) vs gh + Headroom (baseline). Canonical harness-validated campaign.
- **Passes:** 3 (isolated runner per arm×batch×pass; batched Q1-10/Q11-20/Q21-30 per skill).
- **Octocode:** LOCAL monorepo build `packages/octocode/out/octocode.js` (v18.1.1), via `bin/octoc-local`.
- **Baseline:** `gh` 2.96.0 + Headroom 0.34.0 (`bin/ghc`).
- **Runner model:** claude-sonnet-4-6. **Judge model:** gpt-5.5 — neutral, different family; blind X/Y packet, seed=42. (An initial GLM-5.2 judge pass was discarded and re-judged with gpt-5.5.)
- **Measurement:** `total_chars = model-in + model-out` from instrumented per-question JSONL; validated by `validate_campaign.py`.
- **Exclusions (2):** `octocode p1-Q21` and `headroom p3-Q13` failed strict artifact-integrity (re-run call overwrote an artifact) → unresolved, excluded from the tally. 88 scored instances remain.

## Summary (paired, per question)

| Metric | vs Headroom |
|---|---:|
| Correctness — win / tie / loss (octocode) | **60 / 0 / 28** |
| Mean correctness — octocode / baseline | **9.30 / 8.62** (of 10) |
| Char ratio — geo-mean (95% CI) | **2.62×** (1.87–3.71), median 2.77× |
| Questions Octocode leaner | 63 / 88 |
| **Char ratio — geometric mean (headline)** | **2.62× leaner** |
| Char ratio — median (min…max) | 2.77× (0.06 … 1301×) |
| Questions Octocode leaner (of 88) | **63 / 88** |
| Chars pooled sum — *outlier-sensitive* | 17.4× (1.88M vs 32.6M) |

The pooled 17.4× is **not** the headline — it is dominated by three headroom runaways
(`p3-Q17` alone = 20.3M chars; `p3-Q16` = 2.2M; `p1-Q28` = 1.8M), where uncompressed/compressed
`gh` tree+file dumps ballooned. The **geometric mean (2.62×)** and **median (2.77×)** are the
honest per-question figures.

## Best-on (roll-up)

- **Accuracy:** Octocode — higher mean correctness and more wins; decisive on structured-membership
  questions (package.json dep sections, `exports` maps, PR totals) where the compressed-`gh` arm
  repeatedly asserted the wrong package.json shape (Q10, Q14, Q16, Q18) or missing sections.
- **Quality/depth:** Roughly at parity on correctness ceiling; ties were common when both arms
  reached the same facts — Octocode then won on characters.
- **Workflow / characters:** Octocode — leaner in 63/88, targeted region reads vs `gh` whole-file/tree
  fetches; the baseline's worst cases were multi-million-char single questions.

## Fairness caveats

- Two questions excluded for artifact-integrity (above), not for answer quality.
- `min` ratio 0.06 (one instance) = a question where Octocode cloned/read more than a single
  compressed `gh` call; disclosed, not removed.
- Judge is a different model family (gpt-5.5) from both arms to limit self-enhancement bias;
  X/Y randomized per question (seed=42), tool identity redacted.

## Artifacts

- Per-question logs: `campaigns/full-134213-2026-08-07/{octocode,headroom}-p{1,2,3}-Q{1..30}.jsonl`
- Answers: `campaigns/full-134213-2026-08-07/answers/{arm}-p{pass}.md`
- Blind packet + map: `blind-packet.md` (+ `.MAP.secret.txt`)
- Verdicts (gpt-5.5): `campaigns/full-134213-2026-08-07/judge/pass{1,2,3}-verdicts.md`
- Metrics: `campaigns/full-134213-2026-08-07/metrics.json`

## Bottom line

The arms were **not** equally correct — Octocode edged accuracy (9.30 vs 8.62) — and at parity
Octocode was consistently leaner (63/88, ~2.6× geo-mean fewer chars), with the baseline prone to
catastrophic char blowups on tree/file-dump questions. Headline: **more correct and ~2.6× leaner.**
