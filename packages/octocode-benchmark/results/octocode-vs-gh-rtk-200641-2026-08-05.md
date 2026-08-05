# Octocode CLI vs `gh` + `rtk` — 200641 2026-08-05

**Bottom line:** At statistically indistinguishable correctness (paired: B strictly higher on 1 question (Q5), 19 tied; sign test n.s.; means 9.65 A / 9.70 B, near ceiling), **Octocode (B) is the leaner arm** — typically **~3× leaner**: geometric-mean per-question char ratio A/B = **3.11×**, median **3.99×**, leaner on **14/20** questions (sign test **p≈0.12 — not significant at α=0.05 from a single pass**). Aggregation is **per question, paired**, not a pooled sum: the pooled-sum ratio of 5.94× is outlier-inflated — Q16 alone is 29.6% of A's total and the top-5 are 72.4%; dropping Q16 the sum ratio falls to 4.31×. B is heavier on 6/20 questions (ratio range 0.49×–57.9×). B never lost on correctness.

> Method note: aggregation follows `skills/octocode-benchmark/references/aggregation-and-stats.md` — headline the geometric-mean ratio, disclose outliers, treat one pass as a snapshot.
## Run metadata

| Field | Value |
|---|---|
| Matchup | octocode-vs-gh-rtk |
| Question set | `compare/github-questions/` (20 questions), content hash `da3f6b922387` |
| Runner primer | `RUNNER_TOOL_CONTEXT.md` @ git `4d35f0f3` |
| Arm A | read-only `gh` via `rtk gh …` |
| Arm B | `npx octocode tools …` |
| gh | 2.96.0 (2026-07-02), account bgauryy |
| rtk | 0.44.2 |
| octocode | v18.0.1 |
| Measurement | transparent wrapper `.octocode/tmp/measure.sh`, Unicode char count per call → `<RUNDIR>/<A|B>/calls.jsonl` |
| Isolation | 4 runner agents (A:Q1-10, A:Q11-20, B:Q1-10, B:Q11-20), 4 blind judge agents (Q1-5/6-10/11-15/16-20); X/Y randomized per question, tool names hidden |
| Started (UTC) | 2026-08-05 17:06 |

## Per-question table

| Question | Corr A | Corr B | Depth A | Depth B | Wkfl A | Wkfl B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 — Next.js route-regex result | 10 | 10 | 4 | 4 | 5 | 4 | 15,133 | 16,044 | A |
| Q2 — Repository discovery & bounded absence | 10 | 10 | 5 | 3 | 3 | 5 | 76,233 | 11,567 | B |
| Q3 — Flask route history | 10 | 10 | 4 | 4 | 4 | 4 | 34,097 | 16,301 | B |
| Q4 — Axios redirect impl across repos | 10 | 10 | 4 | 4 | 3 | 4 | 155,594 | 35,604 | B |
| Q5 — Vue hydration diff review | 7 | 9 | 3 | 5 | 3 | 5 | 106,318 | 44,426 | B |
| Q6 — Express router cross-repo trace | 9 | 9 | 4 | 5 | 4 | 4 | 51,529 | 20,018 | B |
| Q7 — Zustand Next.js integration contract | 9 | 9 | 4 | 5 | 5 | 4 | 7,461 | 8,371 | A |
| Q8 — VS Code keybinding dispatch | 9 | 9 | 4 | 3 | 3 | 5 | 19,038 | 2,616 | B |
| Q9 — Fastify lifecycle contract | 10 | 10 | 5 | 5 | 2 | 4 | 51,939 | 14,379 | B |
| Q10 — Axios repo & Node entry chain | 9 | 9 | 4 | 5 | 4 | 4 | 10,291 | 10,748 | A |
| Q11 — Esbuild JS-to-Go service boundary | 10 | 10 | 4 | 5 | 3 | 4 | 150,582 | 34,430 | B |
| Q12 — Stream & EventEmitter wiring | 10 | 9 | 5 | 3 | 3 | 4 | 80,581 | 18,098 | B |
| Q13 — Redis BITFIELD security issue & PR | 10 | 10 | 5 | 4 | 4 | 3 | 11,699 | 24,091 | A |
| Q14 — Vitest dependency on Vite | 10 | 10 | 4 | 5 | 5 | 4 | 5,627 | 10,769 | A |
| Q15 — Hono JSX array component PR | 10 | 10 | 5 | 5 | 3 | 5 | 46,267 | 9,335 | B |
| Q16 — ESLint parser dependency chain | 10 | 10 | 4 | 4 | 2 | 4 | 635,931 | 10,984 | B |
| Q17 — Next.js fetch request memoization | 10 | 10 | 4 | 4 | 3 | 4 | 213,346 | 18,398 | B |
| Q18 — Vite dependency-section membership | 10 | 10 | 4 | 4 | 5 | 3 | 10,244 | 12,652 | A |
| Q19 — Node child-process async/sync paths | 10 | 10 | 4 | 4 | 2 | 4 | 396,889 | 29,462 | B |
| Q20 — Actions toolkit exec output path | 10 | 10 | 4 | 4 | 3 | 4 | 66,746 | 13,116 | B |

## Summary of all

| Metric | A (gh + rtk) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 9.65 | 9.70 |
| Research depth (mean) | 4.20 | 4.25 |
| Workflow (mean) | 3.45 | 4.10 |
| **Chars (total)** | 2,145,545 | 361,409 |
| Questions leaner | 6 | 14 |
| Questions preferred | 7 | 13 |

### Robust paired character statistics (headline method)

| Statistic | Value |
|---|---:|
| Geometric-mean per-question ratio A/B | **3.11×** |
| Median per-question ratio A/B | 3.99× (range 0.49×–57.9×) |
| Questions B leaner | 14/20 (sign test p≈0.12, **n.s.**) |
| Pooled sum ratio *(outlier-sensitive)* | 5.94× |
|   top question (Q16) share of A total | 29.6% (top-5 = 72.4%) |
|   leave-one-out (drop Q16) sum ratio | 4.31× |

Overall: correctness is statistically indistinguishable (B strictly higher only on Q5; near-ceiling). **Octocode (B) is the leaner arm — typically ~3× (geometric mean 3.11×, median 3.99×), leaner on 14/20**, but the leaner-count is not significant at α=0.05 from a single pass. The pooled-sum "5.94×" is inflated by one question (Q16) and is reported only as an outlier-disclosed footnote. The per-question preference tally (B 13, A 7) counts efficiency tie-breaks and should not be read as a correctness result.
## Per question (detail)

Correctness verified independently by a blind judge per question (ground truth by its own current-evidence research). Chars are authoritative from the wrapper's `calls.jsonl`.

- **Q1 Next.js `getRouteRegex()`** — both correct: `packages/next/src/shared/lib/router/utils/route-regex.ts`, helper `getParametrizedRoute`, returns `{ re, groups }`. A marginally leaner. **A**
- **Q2 `sindresorhus/is` bounded absence** — both correct (TS, `main`, `isQuantumSuperposition` absent). A(=B arm) confirmed via code search alone; the other read the full 2038-line source (76K chars) for the same NO. Leaner arm wins. **B**
- **Q3 Flask `route`** — both correct: `Scaffold` in `src/flask/sansio/scaffold.py`; commit `705e5268` added `_method_route` + method shortcuts. B leaner. **B**
- **Q4 Axios→follow-redirects** — both correct: `follow-redirects ^1.16.0`, transport-selection branch, `RedirectableRequest._performRequest/_processResponse`. B ~4× leaner. **B**
- **Q5 Vue PR #15035** — B higher correctness (9 vs 7): identified 4 concrete hydration/Vapor scenarios with accurate refs; A covered only 2 core scenarios and mischaracterized the Vapor changes as test rewrites. **B**
- **Q6 Express→pillarjs/router** — both correct: `router ^2.2.0`, `next(err)` advances, `matchLayer` tests one layer. B leaner + deeper. **B**
- **Q7 Zustand contract** — both correct: Context-backed per-request factory; `peerDependenciesMeta.react.optional:true`. A leaner on a tiny question. **A**
- **Q8 VS Code keybinding** — both correct: `WorkbenchKeybindingService` → `AbstractKeybindingService.dispatchEvent`. A far leaner (2.6K vs 19K). **A**
- **Q9 Fastify lifecycle** — both correct incl. `context.onRequest` / `onRequestHookRunner`. A leaner (14K vs 52K raw dumps). **A**
- **Q10 Axios entry chain** — both correct (JS, `main`→exports→`lib/axios.js`). Near-tie chars; slight edge. **A**
- **Q11 Esbuild JS→Go** — both correct incl. Go `runService` in `cmd/esbuild/service.go`, build in separate binary. B ~4.4× leaner. **B**
- **Q12 Stream/EventEmitter** — both essentially correct; the higher-char answer added `_addListener`/`ReflectApply` precision earning a depth/correctness edge here. **A**
- **Q13 Redis BITFIELD** — both correct: issue #15389, `getBitOffsetFromArgument`, PR #15433 (+9/-3, +9/0). Leaner arm won. **A**
- **Q14 Vitest→Vite** — both correct: peer + dev, `optional:false`. A leaner. **A**
- **Q15 Hono PR #5179** — both correct: merged, `src/jsx/base.ts`, issue #5177, alt PR #5178. B ~5× leaner. **B**
- **Q16 ESLint→espree→acorn** — both correct; A(gh arm) ballooned to 636K via a recursive tree listing vs 11K targeted. **B**
- **Q17 Next.js fetch memoization** — both correct on all layers/keys/bypass conditions; B 213K→18K via targeted matchString reads. **B**
- **Q18 Vite dep membership** — both correct across all five sections; A leaner (2 calls). **A**
- **Q19 Node child_process dual path** — both correct; A(gh arm) pulled full raw files (397K) vs 29K targeted. **B**
- **Q20 Actions toolkit `getExecOutput`** — both correct: `ToolRunner.exec`→`child.spawn`, StringDecoder accumulation, `{exitCode,stdout,stderr}`. B leaner. **B**

### Fairness caveats
- The `gh + rtk` arm has no region-targeted file read; on trace questions (Q16/Q17/Q19) it must fetch whole files or recursive trees, which inflates its char count. This is an inherent property of the arm's surface, not a measurement artifact — `search`/`api` are rtk passthrough (no compression).
- Q5 is the only correctness gap (B > A); all other differences are efficiency tie-breaks at equal correctness.
- One pass is a snapshot; repeat for a stable claim.

## Artifacts

- Measurements: `.octocode/tmp/run-200641-2026-08-05/{A,B}/calls.jsonl` (+ `Q*.out.txt` raw output)
- Runner answers: `.octocode/tmp/run-200641-2026-08-05/{A,B}/ans_Q*.md`
- Blind judge packets + scores: `.octocode/tmp/run-200641-2026-08-05/judge/`
- Un-blind map: `.octocode/tmp/run-200641-2026-08-05/judge/unblind_map.json`
