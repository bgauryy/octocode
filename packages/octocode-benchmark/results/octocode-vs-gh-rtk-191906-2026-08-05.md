# Octocode vs `gh` + RTK — full 20-question run, pass 2 (191906, 2026-08-05)

**Bottom line:** Equal-correctness tie — both arms answered all 20 questions correctly
this pass (arm A found the correct Q13 this time). The tie breaks on efficiency:
**Octocode (B) delivers 90.9% fewer characters** (312,323 vs 3,435,499). This is the
second full pass of this matchup on the same day; see the paired note below.

## Run metadata

| Field | Value |
|---|---|
| Comparison | `octocode-vs-gh-rtk` (Arm A = read-only `gh` via `rtk gh`; Arm B = `npx octocode tools …`) |
| Question set | shared `compare/github-questions/` **v2**, 20 questions (commit `4d35f0f3`) |
| `RUNNER_TOOL_CONTEXT.md` commit | `4d35f0f3` |
| Run start | 19:19:06 local · 2026-08-05 |
| Tool versions | `gh` 2.96.0 · `rtk` 0.44.2 · `octocode` v18.0.1 · node v26.4.0 |
| Measurement | transparent wrapper `.octocode/tmp/measure.sh` → per-call Unicode-char log in `calls.jsonl`; artifacts preserved per question |
| Method | Runners A/B ran as isolated blind agents (each given only its arm primer + question); graded against the grader's own primary-evidence research and the pass-1 verified ground truth. |

## Per-question table

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 route-regex | 9 | 9 | 5 | 5 | 4 | 5 | 55,416 | 18,019 | B |
| Q2 is-absence | 9 | 9 | 4 | 4 | 4 | 5 | 10,555 | 24,804 | A |
| Q3 flask-route | 9 | 9 | 5 | 5 | 4 | 5 | 49,286 | 13,301 | B |
| Q4 axios-redirects | 9 | 9 | 5 | 5 | 3 | 5 | 135,733 | 38,985 | B |
| Q5 vue-pr-diff | 9 | 9 | 5 | 5 | 3 | 5 | 150,281 | 19,579 | B |
| Q6 express-router | 9 | 9 | 5 | 5 | 3 | 5 | 121,959 | 9,248 | B |
| Q7 zustand-next | 9 | 9 | 4 | 5 | 4 | 5 | 27,347 | 7,716 | B |
| Q8 vscode-keybind | 9 | 9 | 5 | 4 | 4 | 5 | 38,296 | 2,086 | B |
| Q9 fastify-lifecycle | 9 | 9 | 5 | 5 | 2 | 5 | 247,684 | 18,104 | B |
| Q10 axios-entry | 9 | 9 | 5 | 5 | 2 | 5 | 264,234 | 9,436 | B |
| Q11 esbuild-boundary | 9 | 9 | 5 | 5 | 2 | 4 | 449,354 | 23,220 | B |
| Q12 stream-emitter | 9 | 9 | 5 | 5 | 3 | 5 | 81,676 | 22,331 | B |
| Q13 redis-bitfield | 9 | 9 | 5 | 5 | 5 | 5 | 13,810 | 30,323 | A |
| Q14 vitest-vite | 9 | 9 | 4 | 5 | 5 | 5 | 11,150 | 6,472 | B |
| Q15 hono-jsx-array | 9 | 9 | 4 | 4 | 4 | 5 | 22,830 | 5,878 | B |
| Q16 eslint-parser | 9 | 9 | 4 | 5 | 1 | 5 | 1,261,978 | 10,147 | B |
| Q17 nextjs-fetch-memo | 9 | 9 | 5 | 5 | 3 | 5 | 257,239 | 14,188 | B |
| Q18 vite-dep-membership | 9 | 9 | 4 | 5 | 5 | 5 | 10,244 | 6,174 | B |
| Q19 node-child-process | 9 | 9 | 5 | 5 | 3 | 5 | 201,859 | 15,980 | B |
| Q20 actions-exec | 9 | 9 | 4 | 5 | 4 | 4 | 24,568 | 16,332 | B |

## Summary of all

| Metric | A (`gh`+RTK) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 9.00 | 9.00 |
| Research depth (mean) | 4.65 | 4.85 |
| Workflow (mean) | 3.40 | 4.90 |
| **Chars (total)** | **3,435,499** | **312,323** |
| Questions leaner | 2 | 18 |

Equal correctness; **Octocode 90.9% leaner** and leaner on 18/20 questions.

## Notable points

- **Q13 (Redis BITFIELD) — now a tie.** Unlike pass 1, arm A correctly identified issue
  `#15389` (`#<offset>` parsing overflow in `getBitOffsetFromArgument()`, `bitops.c:730`,
  offset `#144115188075855872`) and fix PR `#15433` (+18/−3). Arm B also correct and additionally
  distinguished the unrelated `#15550`/`#15545` SET-overflow bug. Both fully correct.
- **Workflow gap driver — full-file / recursive-tree pulls.** Arm A's footprint is dominated by
  a few very large raw reads that `gh` cannot region-target: **Q16 = 1,261,978 chars** (recursive
  `git/trees?recursive=1` + full `package.json` reads on the eslint monorepo), Q11 449K, Q10 264K,
  Q9 247K, Q17 257K, Q19 201K. `gh` has no region-targeted read, so exact-membership questions push
  arm A toward whole-file/whole-tree fetches; octocode used `matchString`/line-range reads. This is
  an inherent arm-A constraint, and it is the entire source of the 90.9% gap.
- Structured-fact questions (Q14 vitest, Q18 vite) — both arms read the exact `package.json` and
  agree section-by-section.

## Paired note (2 passes on 2026-08-05)

| Pass | Report | Correctness A/B | Chars A/B | Verdict |
|---|---|---|---|---|
| 1 (183927) | `octocode-vs-gh-rtk-183927-2026-08-05.md` | 8.55 / 9.00 | 1,053,606 / 299,013 | B wins (Q13 + −71.6%) |
| 2 (191906) | this report | 9.00 / 9.00 | 3,435,499 / 312,323 | tie on correctness; B −90.9% |
| **Mean** | — | **8.78 / 9.00** | **2,244,552.5 / 305,668** | **B never loses correctness; B −86.4% chars** |

Across both passes Octocode is at least as correct every time (strictly higher in pass 1 on Q13)
and dramatically leaner. Arm A's aggregate footprint is volatile pass-to-pass because it swings on
how many exact-membership questions force full-file/tree pulls.

## Bottom line

Pass 2 is an equal-correctness tie won by Octocode on efficiency (−90.9% characters, leaner on
18/20). Combined with pass 1 (where Octocode was also strictly more correct via Q13), the two-pass
picture is: **Octocode is never less correct and is 86.4% leaner on average**, with the gap driven
by `gh`'s lack of region-targeted reads on exact-membership questions.
