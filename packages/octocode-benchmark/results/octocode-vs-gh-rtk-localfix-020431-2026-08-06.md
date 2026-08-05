# Octocode CLI (local build w/ prompt+desc fixes) vs `gh` + `rtk` — 020431 2026-08-06

**Bottom line:** Full v2 set, **25 questions × 1 pass**, with arm B running the **local Octocode build** (`node packages/octocode/out/octocode.js`) that embeds the octocode-core prompt + tool-description fixes (verbatim rule; PR-comment-vs-diff rule; value→minify:"none" cue). Octocode was **more correct this pass** — mean **9.72 (B) vs 9.24 (A)**; paired **B 5 wins / 17 ties / 3 losses** (sign test over 8 decisive pairs p≈0.73, not significant) — a reversal of the pre-fix run where A led the paired count 8–3. Octocode stayed the leaner arm: geometric-mean char ratio A/B = **3.24×**, median **3.86×**, leaner on **20/25**. The two failure modes the fixes targeted both improved (Q21, Q25).

> Method note: aggregation per `skills/octocode-benchmark/references/aggregation-and-stats.md` (geometric-mean ratio + outlier disclosure; one pass = snapshot). This is a **local-build probe** to observe the fixes' effect; cross-run correctness deltas are confounded (different judges each pass, single pass, and Q25 ground truth is genuinely contested — see caveats). Not a multi-pass headline.

## Run metadata

| Field | Value |
|---|---|
| Matchup | octocode-vs-gh-rtk (arm B = local build) |
| Question set | `compare/github-questions/` (25, v2), hash `6dcd8c3cefe2` |
| Arm A | read-only `gh` via `rtk gh …` |
| Arm B | `node /Users/bgaryy/code/octocode/packages/octocode/out/octocode.js tools …` (local build, octocode-core fixes) |
| gh / rtk / octocode | 2.96.0 / 0.44.2 / v18.0.1 (local) |
| Measurement | wrapper `.octocode/tmp/measure.sh`, Unicode chars → calls.jsonl |
| Isolation | 4 runner agents (A/B × Q1-13/Q14-25), 5 blind judges (5 Q each); X/Y randomized, tool identities sanitized (incl. local-bin path), verified no residual tells |
| Passes | 1 (snapshot) |
| Started (UTC) | 2026-08-05 23:04 |

## Per-question table

| Question | Corr A | Corr B | Chars A | Chars B | Ratio A/B | Leaner |
|---|---:|---:|---:|---:|---:|---|
| Q1 route-regex | 10 | 10 | 41,912 | 18,209 | 2.30 | B |
| Q2 repo/absence | 10 | 10 | 75,761 | 56,857 | 1.33 | B |
| Q3 Flask history | 10 | 10 | 40,173 | 6,202 | 6.48 | B |
| Q4 Axios redirect | 9 | 10 | 292,808 | 28,201 | 10.38 | B |
| Q5 Vue PR diff | 8 | 9 | 18,986 | 42,115 | 0.45 | A |
| Q6 Express trace | 10 | 9 | 51,055 | 12,690 | 4.02 | B |
| Q7 Zustand contract | 10 | 10 | 13,476 | 5,537 | 2.43 | B |
| Q8 VS Code keybinding | 10 | 9 | 59,019 | 2,084 | 28.32 | B |
| Q9 Fastify lifecycle | 10 | 10 | 152,003 | 10,903 | 13.94 | B |
| Q10 Axios entry chain | 10 | 10 | 7,616 | 10,192 | 0.75 | A |
| Q11 Esbuild boundary | 9 | 9 | 230,059 | 18,100 | 12.71 | B |
| Q12 Stream/EventEmitter | 10 | 9 | 154,646 | 19,492 | 7.93 | B |
| Q13 Redis BITFIELD | 2 | 10 | 4,379 | 16,356 | 0.27 | A |
| Q14 Vitest→Vite | 10 | 10 | 16,725 | 16,456 | 1.02 | B |
| Q15 Hono PR | 10 | 10 | 3,772 | 6,177 | 0.61 | A |
| Q16 ESLint parser chain | 9 | 10 | 11,648 | 10,909 | 1.07 | B |
| Q17 Next.js fetch memo | 10 | 10 | 160,249 | 22,579 | 7.10 | B |
| Q18 Vite dep sections | 10 | 10 | 5,122 | 15,082 | 0.34 | A |
| Q19 Node child-process | 10 | 10 | 176,675 | 45,787 | 3.86 | B |
| Q20 Actions toolkit exec | 10 | 10 | 24,882 | 21,670 | 1.15 | B |
| Q21 LangChain flow+graph | 9 | 9 | 371,010 | 161,545 | 2.30 | B |
| Q22 Axios tag compare | 10 | 10 | 1,475,228 | 40,194 | 36.70 | B |
| Q23 Linux write() VFS | 10 | 10 | 224,127 | 31,856 | 7.04 | B |
| Q24 Axios buildFullPath | 10 | 10 | 130,302 | 12,485 | 10.44 | B |
| Q25 Axios PR patch+review | 5 | 9 | 320,843 | 18,002 | 17.82 | B |

## Summary

| Metric | A (gh + rtk) | B (Octocode local) |
|---|---:|---:|
| Correctness — paired win/tie/loss (B vs A) | — | **5 / 17 / 3** |
| Correctness (mean) | 9.24 | **9.72** |
| Sign test (8 decisive) | — | p≈0.73 (n.s.) |
| **Char ratio A/B — geometric mean** | — | **3.24×** |
| Char ratio A/B — median (min…max) | — | 3.86× (0.27×…36.70×) |
| Questions B leaner (of 25) | 5 | **20** |
| Chars pooled sum — *outlier-sensitive* | 4,062,476 | 649,680 (6.25×) |
|   heaviest A share; leave-one-out ratio | Q22 = 36.3% | drop Q22 → 4.24× |

## Effect of the fixes (vs the pre-fix run 011611)

- **Q21 (LangChain node value):** pre-fix B lost (7 vs 9) by reporting node value `"agent"`; now **tie 9/9** — B reported the verbatim node names `"model_request"`/`"tools"`. Consistent with the new "copy the exact bytes, never paraphrase" rule.
- **Q25 (PR landed wording):** pre-fix B lost (5 vs 9); now **B 9 / A 5** — this pass A concluded the emoji was *not* restored while the merged diff added a new `## 🔥 HTTP/2 Support` section; B was graded correct on the landed diff. (See caveat — Q25 ground truth is contested across passes.)
- **Q13 (Redis BITFIELD):** B correct both runs (right issue/PR #15389/#15433); A wrong both runs (picked the related #15550/#15545).
- Net paired direction flipped from A-favored (pre-fix 8–3) to **B-favored (5–3)**.

## Caveats (do not over-claim)
- **Single pass, different judges each run** — the +0.48 mean correctness and the direction flip are **not statistically significant** (p≈0.73) and are partly judge-variance, not purely the fixes.
- **Q25 is genuinely contested:** the PR adds a `## 🔥 HTTP/2 Support` section *and* renames the reviewed heading to `#### HTTP/2 Support` (no emoji). Different judges legitimately reached opposite "was the emoji restored?" verdicts across the two runs. Treat Q25 as ambiguous, not as clean evidence either way.
- Char pooled sum is dominated by Q22 (36.3% of A); headline is the geometric mean (3.24×).
- A was leaner on 5 small package.json/entry questions (Q5, Q10, Q15, Q18) where one raw file fetch beats a multi-call read.

## Bottom line
With the local fixes, Octocode was **at least as correct on every question except three (Q6/Q8/Q12 completeness edges), net more correct on the paired count**, and **~3.2× leaner** (20/25). The two targeted grounding failures (Q21 verbatim value, Q25 diff-over-comment) improved in the expected direction. Because it's a single pass with judge variance and a contested Q25, this is **encouraging directional evidence, not proof** — run ≥3 passes with a fixed rubric to confirm.
