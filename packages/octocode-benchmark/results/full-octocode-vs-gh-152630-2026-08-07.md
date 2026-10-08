# Full campaign — Octocode (local build) vs plain `gh`

**Outcome:** Against the **uncompressed, undisciplined-free baseline** — plain read-only GitHub
CLI taking its leanest legal path — Octocode reaches **the same correctness** while using
**~2.0× fewer characters through the model** (geometric mean of the per-question ratio, 95% CI
1.52–2.61), and was leaner on **67 of 89** scored question-instances (sign test p < 0.0001).
Correctness was a near-tie (octocode 9.19 vs gh 9.27 / 10); the judge decided most questions on
characters, so Octocode won **51 of 89** on a correct-and-leaner basis (gh 38). Plain `gh` is the
**leanest of the three baselines** (geo-mean 1.99× here vs 2.62× Headroom, 3.22× RTK) — a bare
`gh` snippet search with minimal `--json` fields is already fairly tight — yet Octocode is still
reliably ~2× leaner at equal accuracy.

## Run metadata

- **Question set:** `compare/github-questions/` v2, 30 questions.
- **Matchup:** Octocode (anchor) vs plain `gh` (baseline). Runnable via `compare/octocode-vs-gh/`.
- **Passes:** 3 (isolated runner per arm×batch×pass; batched Q1-15/Q16-30 per arm).
- **Octocode arm:** LOCAL monorepo build `packages/octocode/out/octocode.js` (v18.1.1), via `../bin/octoc-local`. Anchor runs reused from the octocode-vs-headroom campaign (identical tool/build/questions) for cross-matchup comparability.
- **Baseline:** `gh` 2.96.0, read-only, via the instrumented wrapper `compare/bin/ghm` (bare `gh <args>`, no transport wrapper, no compression).
- **Runner model:** claude-sonnet-4-6. **Judge model:** gpt-5.5 (guy-provider-openai) — neutral, different family from the runner; blind X/Y packet, seed=42, tool identity redacted.
- **Measurement:** `total_chars = model-in + model-out` from instrumented per-question JSONL.

## Summary (paired, per question)

| Metric | vs plain gh |
|---|---:|
| Correctness — win / tie / loss (octocode) | 16 / 51 / 22 |
| Mean correctness — octocode / baseline | 9.19 / 9.27 (of 10) |
| Correct-and-leaner wins (octocode / gh) | **51 / 38** |
| **Char ratio — geometric mean (headline)** | **1.99× leaner** (95% CI 1.52–2.61) |
| Char ratio — median (min…max) | 2.16× (0.03 … 34.93×) |
| Questions Octocode leaner (of 89) | **67 / 89** (sign test p < 0.0001) |
| Depth / Workflow mean (octo / gh) | 4.55 / 4.58 · **4.37 / 3.85** |
| Chars pooled sum — *outlier-sensitive* | 5.16× (9.79M vs 1.90M); top question 20.4%, leave-one-out 4.23× |

The pooled 5.16× is **not** the headline — it is outlier-sensitive (top question = 20.4% of the
gh total; leave-one-out 4.23×). The **geometric mean (1.99×)** and **median (2.16×)** are the
honest per-question figures. Octocode's edge shows most in **workflow** (4.37 vs 3.85): `gh` has
no server-side region read, so whole-file `raw` fetches inflate its footprint where Octocode does
a targeted region/symbol read.

## Best-on (roll-up)

- **Accuracy:** ~parity (9.19 vs 9.27). Both arms reach the same core facts on most questions
  (51 ties); gh's small edge came from a few verbatim-detail captures, Octocode's from
  structured-membership reads.
- **Workflow / characters:** Octocode — leaner on 67/89 at a typical ~2× fewer characters, via
  targeted region/symbol reads vs `gh` whole-file `raw` fetches; the gap widens on large-file,
  multi-hop questions.
- **Overall:** same-tier accuracy, ~2× less context — the win is smaller than vs RTK/Headroom
  because bare disciplined `gh` is the leanest baseline, but it remains reliable (CI above 1×,
  p < 0.0001).

## Fairness caveats

- Anchor (octocode) runs reused across matchups; `octocode p1-Q21` still carries the prior
  artifact-integrity caveat from the headroom campaign → **excluded** from the tally (89 scored
  instances remain), consistent with the RTK/Headroom reports.
- `min` ratio 0.03 = one question where Octocode read/cloned far more than a single `gh` call;
  disclosed, not removed.
- Judge is a different model family (gpt-5.5) from the runner to limit self-enhancement bias; X/Y
  randomized per question (seed=42), tool identity redacted; the judge established ground truth
  independently with read-only `gh`.

## Artifacts

- Logs: `campaigns/full-gh-143806-2026-08-07/{octocode,gh}-p{1,2,3}-Q{1..30}.jsonl`
- Answers: `.../answers/{arm}-p{pass}.md`
- Blind packet + map: `.../blind-packet.md` (+ `.MAP.secret.txt`)
- Verdicts (gpt-5.5): `.../judge/pass{1,2,3}-verdicts.md`
- Metrics + aggregation: `.../metrics.json`, `.../aggregate.py`, `.../aggregate-out.json`
