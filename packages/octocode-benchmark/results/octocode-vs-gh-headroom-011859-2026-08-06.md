# Octocode vs `gh` + Headroom — 2026-08-06

**Outcome: Octocode (B) wins.** At statistically-indistinguishable correctness
(paired sign test p≈0.15), Octocode delivered the same answers into context for a
**geometric-mean 5.4× fewer characters** (median 6.1×), and was leaner on **22 of 25**
questions (sign test p≈0.00016). Where correctness *did* separate the arms it favored
Octocode net (**9 B-wins / 13 ties / 3 B-losses**): the `gh`+Headroom arm produced two
confidently-wrong structured-fact answers (Q14, Q18) and one badly incomplete trace (Q11);
Octocode produced one confidently-wrong answer (Q21, wrong LangGraph node name).

One pass — a snapshot, no within-question variance. Repeat ≥3× for a stable, significant
correctness claim.

## Run metadata

| Field | Value |
|---|---|
| Question set | `compare/github-questions/` (25 questions), repo commit `bc00ea02` |
| `RUNNER_TOOL_CONTEXT.md` | commit `4d35f0f3` |
| Arm A | read-only `gh` 2.96.0 via `./bin/ghc`, Headroom **0.34.0**, model `kompress-v2-base` (onnx) |
| Arm B | `npx octocode tools …` (octocode **v18.0.1**) via `./bin/octoc` |
| Preflight | `preflight.py --warmup` PASS (valid no-op + SmartCrusher + Kompress paths) |
| Passes | 1 (snapshot) |
| Measurement | chars-in from instrumented logs (`bin/sumlog.py --strict` for A; `chars` field for B), never self-report |
| Judging | one blind judge per question, X/Y tool-labels stripped, randomized position, independent ground-truth research |

> Version note: README pins Headroom 0.33.0; this run used 0.34.0 (same `kompress-v2-base`).
> A different Headroom/model version is a different arm A — do not compare across versions.

## Per-question table

| Question | Corr A | Corr B | Depth A | Depth B | WF A | WF B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 | 9 | 9 | 3 | 3 | 4 | 3 | 9,477 | 15,046 | A |
| Q2 | 9 | 9 | 2 | 3 | 3 | 3 | 181 | 4,644 | A |
| Q3 | 9 | 9 | 3 | 3 | 2 | 4 | 59,885 | 8,741 | B |
| Q4 | 7 | 9 | 4 | 3 | 2 | 4 | 131,968 | 25,359 | B |
| Q5 | 9 | 8 | 4 | 3 | 3 | 4 | 88,974 | 19,585 | B |
| Q6 | 10 | 10 | 4 | 4 | 4 | 4 | 58,587 | 17,991 | B |
| Q7 | 7 | 10 | 4 | 5 | 4 | 5 | 38,571 | 9,742 | B |
| Q8 | 10 | 10 | 4 | 4 | 4 | 4 | 58,507 | 2,010 | B |
| Q9 | 10 | 10 | 5 | 5 | 4 | 5 | 260,676 | 21,404 | B |
| Q10 | 5 | 9 | 4 | 4 | 3 | 4 | 18,452 | 3,748 | B |
| Q11 | 5 | 9 | 3 | 4 | 3 | 4 | 275,170 | 21,040 | B |
| Q12 | 9 | 9 | 5 | 4 | 4 | 4 | 86,209 | 10,473 | B |
| Q13 | 9 | 9 | 5 | 5 | 4 | 4 | 513,220 | 16,029 | B |
| Q14 | 1 | 10 | 2 | 4 | 2 | 5 | 33,741 | 6,476 | B |
| Q15 | 10 | 9 | 4 | 3 | 4 | 4 | 4,351 | 5,998 | A |
| Q16 | 10 | 10 | 4 | 4 | 1 | 5 | 100,014 | 9,621 | B |
| Q17 | 10 | 10 | 5 | 5 | 4 | 5 | 267,080 | 9,206 | B |
| Q18 | 2 | 10 | 3 | 5 | 2 | 5 | 81,667 | 7,364 | B |
| Q19 | 9 | 9 | 4 | 4 | 4 | 4 | 173,022 | 27,800 | B |
| Q20 | 9 | 9 | 5 | 4 | 4 | 4 | 45,588 | 14,980 | B |
| Q21 | 9 | 5 | 4 | 4 | 3 | 4 | 307,534 | 52,537 | B |
| Q22 | 5 | 7 | 4 | 4 | 3 | 4 | 290,584 | 18,158 | B |
| Q23 | 9 | 9 | 5 | 4 | 3 | 4 | 140,494 | 23,083 | B |
| Q24 | 8 | 9 | 4 | 4 | 3 | 4 | 71,049 | 13,817 | B |
| Q25 | 6 | 7 | 4 | 4 | 3 | 4 | 132,494 | 15,424 | B |

## Summary (aggregated per question, paired)

| Metric | A (`gh`+Headroom) | B (Octocode) |
|---|---:|---:|
| Correctness — paired win/tie/loss (B view) | 3 wins / 13 ties / **9 losses** | **9 wins** / 13 ties / 3 losses |
| Correctness sign test (12 decisive, B 9) | — | p ≈ 0.15 (indistinguishable) |
| Correctness (mean, secondary — ceiling) | 7.84 | **8.96** |
| Research depth (mean) | 3.92 | 3.96 |
| Workflow (mean) | 3.20 | **4.16** |
| **Char ratio A/B — geometric mean** (headline) | — | **5.38× leaner (B)** |
| Char ratio A/B — median (min…max) | — | 6.09× (0.04×…32.02×) |
| Questions B leaner (of 25) + sign test | 3 | **22** (p ≈ 0.00016) |
| Chars pooled sum — *outlier-sensitive* | 3,247,495 | 380,276 |
| top-A share; leave-one-out geomean | Q13 = 15.8% of A | LOO 4.99×…6.61× |

Correctness is at ceiling and statistically tied, so per `SCORING.md` **efficiency
decides — and Octocode wins the tiebreaker decisively** (5.4× geomean, leaner on 22/25).
The pooled 8.5× is outlier-sensitive; the leave-one-out band (5.0–6.6×) around the geomean
is the honest headline. Only 3 questions favored `gh`+Headroom on characters — the tiny
Q2 bounded-absence check (181 vs 4,644), Q1 (a single compressed tree fetch), and Q15.

## Notable per-question findings

- **Q14 / Q18 (structured `package.json` membership):** arm A wrong. Headroom's lossy
  compression of the full `package.json` elided section boundaries, so the runner could not
  distinguish `dependencies` vs `peerDependencies`/`peerDependenciesMeta.optional` and
  inferred incorrectly (Q14 correctness 1; Q18 correctness 2). Octocode's exact
  `minify:"none"` read answered both cleanly (10/10). This is the core risk of a
  compression-only transport for exact field-membership questions.
- **Q11 (esbuild JS→Go boundary):** arm A incomplete (5/10) after ~275K chars and a stuck
  full-file crawl; Octocode traced entry→channel→Go `runService()` in 21K chars.
- **Q21 (LangChain `createAgent` graph):** **arm B wrong (5/10)** — Octocode reported the
  model node as `"agent_request"`; primary source (`AgentNode.ts`) is
  `AGENT_NODE_NAME = "model_request"`, which arm A got right. A genuine Octocode miss.
- **Q22 (axios v1.18.0…v1.19.0):** arm B cited a non-existent SHA (hallucination, 7/10);
  arm A cited a real SHA with a minor diff transcription slip (5/10) — both flawed.
- **Q16 workflow:** arm A took 57 calls (workflow 1) vs Octocode's 5.

## Fairness caveats

- Headroom is a transport/compression layer only; arm A's GitHub reach equals plain `gh`.
  Its lossy prose/JSON path is exactly what hurt Q14/Q18 — a real property of the arm, not a
  harness error.
- Arm B had 7 failed (exit≠0) tool calls across the campaign (early schema/quoting retries);
  their output was captured and counts toward B's chars/workflow as measured waste.
- Single pass; neural compression path can drift across machines/model revisions.

## Bottom line

The two arms are **equally correct within noise**, and Octocode is **~5.4× leaner in
delivered characters** (median 6.1×, leaner on 22/25, p≈0.00016). Compressing `gh` output
recovers a large slice of the context gap but **loses exact structured-fact fidelity**
(Q14, Q18) — the one place a lossy transport cannot substitute for an exact read.
