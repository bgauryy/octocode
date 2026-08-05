# Octocode CLI vs `gh` + `rtk` — 011611 2026-08-06

**Bottom line:** Full v2 set, **25 questions × 1 pass**. Correctness was **statistically indistinguishable** (means A 9.24 / B 9.18; paired B wins 3, A wins 8, 14 ties; sign test over 11 decisive pairs **p≈0.23 — not significant**, near ceiling). **Octocode (B) is the leaner arm** — geometric-mean per-question char ratio A/B = **4.98×**, median **6.65×**, leaner on **21/25** questions. Aggregation is **per question, paired**; the pooled-sum ratio 11.54× is outlier-sensitive (heaviest question Q22 = 24.8% of A's chars; leave-one-out 9.40×). Neither arm made a *net* correctness win; each has distinct failure modes (below).

> Method note: aggregation follows `skills/octocode-benchmark/references/aggregation-and-stats.md` — headline the geometric-mean ratio, disclose outliers, treat one pass as a snapshot. This run includes the 5 new advanced questions (Q21–Q25), so it supersedes the earlier 20-question v2 headline.

## Run metadata

| Field | Value |
|---|---|
| Matchup | octocode-vs-gh-rtk |
| Question set | `compare/github-questions/` (25 questions, v2), content hash `6dcd8c3cefe2` |
| Runner primer | `RUNNER_TOOL_CONTEXT.md` @ git `4d35f0f3` |
| Arm A | read-only `gh` via `rtk gh …` |
| Arm B | `npx octocode tools …` |
| gh | 2.96.0 (2026-07-02), account bgauryy |
| rtk | 0.44.2 |
| octocode | v18.0.1 |
| Measurement | transparent wrapper `.octocode/tmp/measure.sh`, Unicode char count per call → `<RUNDIR>/<A\|B>/calls.jsonl` |
| Isolation | 4 runner agents (A:Q1-13, A:Q14-25, B:Q1-13, B:Q14-25), 5 blind judge agents (Q1-5/6-10/11-15/16-20/21-25); X/Y randomized per question, tool identities sanitized (verified no residual tells) |
| Passes | 1 (single-pass snapshot) |
| Started (UTC) | 2026-08-05 22:16 |

## Per-question table

| Question | Corr A | Corr B | Chars A | Chars B | Ratio A/B | Leaner |
|---|---:|---:|---:|---:|---:|---|
| Q1 — Next.js route-regex result | 10 | 9 | 52,008 | 15,913 | 3.27 | B |
| Q2 — Repo discovery & bounded absence | 10 | 10 | 75,759 | 15,272 | 4.96 | B |
| Q3 — Flask route history | 10 | 10 | 39,931 | 9,023 | 4.43 | B |
| Q4 — Axios redirect impl across repos | 10 | 10 | 324,005 | 24,038 | 13.48 | B |
| Q5 — Vue hydration diff review | 8 | 10 | 35,296 | 19,762 | 1.79 | B |
| Q6 — Express router cross-repo trace | 10 | 10 | 57,447 | 13,372 | 4.30 | B |
| Q7 — Zustand Next.js integration contract | 10 | 10 | 8,034 | 4,571 | 1.76 | B |
| Q8 — VS Code keybinding dispatch | 10 | 10 | 19,596 | 13,753 | 1.42 | B |
| Q9 — Fastify lifecycle contract | 10 | 10 | 152,477 | 10,746 | 14.19 | B |
| Q10 — Axios repo & Node entry chain | 10 | 9 | 10,173 | 14,789 | 0.69 | A |
| Q11 — Esbuild JS-to-Go service boundary | 9.5 | 9 | 357,907 | 24,875 | 14.39 | B |
| Q12 — Stream & EventEmitter wiring | 10 | 9.5 | 81,449 | 10,563 | 7.71 | B |
| Q13 — Redis BITFIELD security issue & PR | 3 | 10 | 160,479 | 18,933 | 8.48 | B |
| Q14 — Vitest dependency on Vite | 6 | 6 | 11,150 | 30,573 | 0.36 | A |
| Q15 — Hono JSX array component PR | 9.5 | 9.5 | 3,772 | 6,177 | 0.61 | A |
| Q16 — ESLint parser dependency chain | 10 | 10 | 652,826 | 18,448 | 35.39 | B |
| Q17 — Next.js fetch request memoization | 10 | 10 | 423,629 | 13,031 | 32.51 | B |
| Q18 — Vite dependency-section membership | 10 | 10 | 10,244 | 26,692 | 0.38 | A |
| Q19 — Node child-process async/sync paths | 9.5 | 8 | 179,940 | 20,734 | 8.68 | B |
| Q20 — Actions toolkit exec output path | 10 | 10 | 143,185 | 16,333 | 8.77 | B |
| Q21 — LangChain createAgent flow & graph | 9 | 7 | 1,362,853 | 106,027 | 12.85 | B |
| Q22 — Axios v1.18.0..v1.19.0 range compare | 8 | 8.5 | 1,475,228 | 39,804 | 37.06 | B |
| Q23 — Linux write() syscall to VFS flow | 9.5 | 9 | 134,366 | 8,683 | 15.47 | B |
| Q24 — Axios buildFullPath blast radius | 10 | 10 | 84,777 | 12,744 | 6.65 | B |
| Q25 — Axios PR selected-patch + review | 9 | 5 | 85,993 | 20,308 | 4.23 | B |

## Summary of all

| Metric | A (gh + rtk) | B (Octocode) |
|---|---:|---:|
| Correctness — paired win/tie/loss (B vs A) | — | 3 / 14 / 8 |
| Correctness (mean, secondary — near ceiling) | **9.24** | 9.18 |
| Sign test (11 decisive pairs) | — | two-sided **p≈0.23 (n.s.)** |
| **Char ratio A/B — geometric mean** (headline) | — | **4.98×** |
| Char ratio A/B — median (min…max) | — | 6.65× (0.36×…37.06×) |
| Questions B leaner (of 25) | 4 | **21** |
| Chars pooled sum — *outlier-sensitive* | 5,942,524 | 515,164 (11.54×) |
|   heaviest A question share; leave-one-out ratio | Q22 = 24.8% | drop Q22 → 9.40× |

## Correctness splits (where the arms differed)

**Octocode (B) won:**
- **Q13 (A=3, B=10)** — the decisive miss: baseline identified a *related* Redis bug (issue #15550 / PR #15545, signed `SET`/`INCRBY` overflow) instead of the `#<offset>` **parsing** overflow the question asks for (issue **#15389** / PR **#15433**, `getBitOffsetFromArgument` `loffset *= bits`). Octocode nailed it.
- **Q5 (A=8, B=10)** — Octocode read both source files and traced exact hydration/vapor edits; baseline's second scenario was vaguer/test-derived.
- **Q22 (A=8, B=8.5)** — baseline fabricated an inline `// ← the fix` annotation and dropped real hunk lines; Octocode kept the hunk faithful.

**Baseline (A) won:**
- **Q25 (A=9, B=5)** — the review-thread trap: the emoji was **not** restored at the reviewed heading (merged as `#### HTTP/2 Support`, no emoji). Octocode fabricated a resolving quote and wrongly claimed the emoji request was satisfied; baseline read the merged diff correctly.
- **Q21 (A=9, B=7)** — Octocode gave the model-request node value as `"agent"`; truth/baseline = `"model_request"`.
- **Q19 (A=9.5, B=8)** — Octocode misnamed the async spawn dispatch; baseline had `_handle.spawn()` via `internalBinding('process_wrap')`.
- Q1, Q10, Q11, Q12, Q23 — baseline marginally more complete (line numbers / requested language breakdown / extra verified detail).

Both arms shared the same Q14 error (labelled Vitest's `devDependencies` vite entry as a regular dependency; both correct that the peer is not optional) → 6/6.

## Fairness caveats
- Single pass, n=25 — no within-question variance; the paired correctness edge to A (8 vs 3) is **not significant** (p≈0.23). Treat as a snapshot.
- Char pooled-sum is dominated by two heavy baseline questions (Q21 1.36M, Q22 1.48M = together 48% of A's total); headline is the geometric mean (4.98×), not the sum.
- Octocode was **heavier** on 4 small questions (Q10, Q14, Q15, Q18) — mostly package.json/entry-chain reads where a single raw `gh` file fetch is competitive.

## Bottom line
On the full 25-question v2 set the two arms were **equally correct within noise (A 9.24 / B 9.18, n.s.)**, and **Octocode delivered the answer with ~5× fewer characters (geo-mean), leaner on 21/25**. The result is a leanness win at parity correctness — not a correctness win. The advanced questions (Q21–Q25) are the most discriminating: they produced the sharpest splits in both directions (Octocode's Q22 faithful-hunk win; baseline's Q21/Q25 accuracy wins), confirming they surface real, fixable Octocode weaknesses (graph node values, trusting a PR comment over the merged diff) that the easier questions mask.
