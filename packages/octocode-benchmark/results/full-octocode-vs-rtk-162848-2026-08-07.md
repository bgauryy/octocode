# Full campaign — Octocode (local build) vs gh + RTK

**Outcome:** At near-identical correctness, Octocode is **markedly leaner** than gh+RTK across the
full 30-question v2 GitHub set over 3 passes. After excluding the one shared local-build
artifact-integrity failure (`octocode-p1-Q21`), Octocode won 56 of 89 scored question-instances
(rtk 33), with essentially equal mean correctness (9.29 vs 9.42/10) while using **~3.2× fewer
characters through the model** (geometric mean of the per-question ratio). Because correctness was
a near-tie on almost every question, the judge decided most questions on characters — and Octocode
was leaner in 67/89.

## Run metadata

- **Question set:** `compare/github-questions/` v2, 30 questions.
- **Matchup:** Octocode (anchor) vs gh + RTK (baseline).
- **Passes:** 3 (isolated runner per arm×batch×pass; batched Q1-10/Q11-20/Q21-30 per skill).
- **Octocode arm:** LOCAL monorepo build `packages/octocode/out/octocode.js` (v18.1.1), via `bin/octoc-local`. Anchor runs reused from the octocode-vs-headroom campaign (identical tool/build/questions) for cross-matchup comparability.
- **Baseline:** `gh` 2.96.0 + `rtk` 0.44.2 (`bin/rtkm`, `rtk gh <args>`).
- **Runner model:** claude-sonnet-4-6. **Judge model:** gpt-5.5 (guy-provider-openai) — neutral, different family from the runners; blind X/Y packet, seed=42.
- **Measurement:** `total_chars = model-in + model-out` from instrumented per-question JSONL.

## Summary (paired, per question)

| Metric | vs RTK |
|---|---:|
| Correctness — win / tie / loss (octocode) | **56 / 0 / 33** |
| Mean correctness — octocode / baseline | 9.29 / 9.42 (of 10) |
| **Char ratio — geometric mean (headline)** | **3.21× leaner** |
| Char ratio — median (min…max) | 2.83× (0.02 … 221×) |
| Questions Octocode leaner (of 89) | **67 / 89** |
| Chars pooled sum — *outlier-sensitive* | 16.7× (1.99M vs 33.2M) |

The judge (gpt-5.5) emitted a winner on every question (no explicit ties): where correctness was
equal — the common case — it decided on fewer characters, so the win column reflects
"correct-and-leaner". RTK edged raw mean correctness by 0.1 (a handful of questions where the rtk
runner captured an extra verbatim detail), but never at a lower character cost.

## Best-on (roll-up)

- **Accuracy:** ~parity (9.29 vs 9.42). Both arms reach the same core facts on most questions; RTK's
  tiny edge came from a few verbatim-detail captures, Octocode's from structured-membership reads.
- **Workflow / characters:** Octocode — leaner in 67/89, ~3.2× fewer chars (geo-mean). RTK relays
  raw `gh` output uncompressed, so tree/file-heavy questions cost far more; the worst ratio was 221×.
- **Overall:** Octocode wins the matchup on leanness at equal correctness.

## Fairness caveats

- Anchor (octocode) runs reused across matchups; `octocode p1-Q21` carries the prior
  artifact-integrity caveat from the headroom campaign and is excluded from scored-instance
  counts here, matching `SUMMARY.md` and `VALIDATION-2026-08-08.md`.
- `min` ratio 0.02 = one question where Octocode read/cloned far more than a single `gh` call; disclosed.
- Judge is a different model family (gpt-5.5) from both arms to limit self-enhancement bias; X/Y
  randomized per question (seed=42), tool identity redacted. (An earlier GLM-5.2 judge pass was
  discarded at request and re-run with gpt-5.5.)

## Artifacts

- Logs: `campaigns/full-rtk-162848-2026-08-07/{octocode,rtk}-p{1,2,3}-Q{1..30}.jsonl`
- Answers: `.../answers/{arm}-p{pass}.md`
- Blind packet + map: `.../blind-packet.md` (+ `.MAP.secret.txt`)
- Verdicts (gpt-5.5): `.../judge/pass{1,2,3}-verdicts.md`
- Metrics: `.../metrics.json`

## Bottom line

The arms were **effectively equally correct** (9.29 vs 9.42); at that parity Octocode was
consistently and substantially **leaner (~3.2× geo-mean, 67/89)**. Headline: **same accuracy, ~3.2× fewer characters.**
