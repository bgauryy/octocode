# Octocode vs `gh` + RTK — full 20-question run (183927, 2026-08-05)

**Bottom line:** Octocode (B) wins this snapshot on **both** axes. Arms tie on 18 of 20
questions; on **Q13** arm A gives a confidently-wrong answer (it answered a *different*
Redis bug) while B is correct, so **B is higher on correctness**. Across all 20 questions
B also delivers **71.6% fewer characters** into context (299,013 vs 1,053,606). One full
pass is a snapshot — repeat for a stable claim.

## Run metadata

| Field | Value |
|---|---|
| Comparison | `octocode-vs-gh-rtk` (Arm A = read-only `gh` via `rtk gh`; Arm B = `npx octocode tools …`) |
| Question set | shared `compare/github-questions/` **v2**, 20 questions |
| Question-set commit | `4d35f0f3` |
| `RUNNER_TOOL_CONTEXT.md` commit | `4d35f0f3` |
| Run start | 18:39:27 local · 2026-08-05 (verification ~15:55 UTC) |
| Tool versions | `gh` 2.96.0 · `rtk` 0.44.2 · `octocode` v18.0.1 · node v26.4.0 |
| Measurement | transparent wrapper `.octocode/tmp/measure.sh` logging Unicode chars of raw CLI output per call to `calls.jsonl`; artifacts preserved per question |
| Method note | Runners A and B ran as isolated blind agents (each given only its arm primer + question). Grading is against the grader's own primary-evidence research. |

## Per-question table

| Question | Correctness A | Correctness B | Depth A | Depth B | Workflow A | Workflow B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 route-regex | 9 | 9 | 4 | 5 | 5 | 5 | 16,451 | 14,013 | B |
| Q2 is-absence | 9 | 9 | 4 | 4 | 5 | 4 | 708 | 27,502 | A |
| Q3 flask-route | 9 | 9 | 4 | 5 | 4 | 5 | 40,424 | 19,229 | B |
| Q4 axios-redirects | 9 | 9 | 5 | 5 | 4 | 5 | 78,874 | 32,849 | B |
| Q5 vue-pr-diff | 8 | 9 | 4 | 5 | 5 | 5 | 18,516 | 23,772 | A |
| Q6 express-router | 9 | 9 | 5 | 5 | 5 | 5 | 18,715 | 7,964 | B |
| Q7 zustand-next | 9 | 9 | 4 | 5 | 5 | 5 | 7,461 | 4,235 | B |
| Q8 vscode-keybind | 9 | 9 | 4 | 4 | 5 | 5 | 19,038 | 2,022 | B |
| Q9 fastify-lifecycle | 9 | 9 | 5 | 5 | 5 | 5 | 5,056 | 10,749 | A |
| Q10 axios-entry | 9 | 9 | 4 | 5 | 5 | 5 | 3,433 | 20,211 | A |
| Q11 esbuild-boundary | 9 | 9 | 5 | 5 | 3 | 5 | 228,759 | 34,159 | B |
| Q12 stream-emitter | 9 | 9 | 5 | 5 | 4 | 5 | 82,404 | 10,375 | B |
| **Q13 redis-bitfield** | **1** | **9** | 2 | 5 | 2 | 5 | 5,117 | 16,620 | A |
| Q14 vitest-vite | 9 | 9 | 4 | 5 | 5 | 5 | 11,150 | 6,472 | B |
| Q15 hono-jsx-array | 9 | 9 | 4 | 4 | 5 | 5 | 3,343 | 5,878 | A |
| Q16 eslint-parser | 9 | 9 | 5 | 5 | 5 | 5 | 28,277 | 10,147 | B |
| Q17 nextjs-fetch-memo | 9 | 9 | 5 | 5 | 3 | 5 | 256,569 | 12,274 | B |
| Q18 vite-dep-membership | 9 | 9 | 4 | 5 | 5 | 5 | 5,122 | 10,420 | A |
| Q19 node-child-process | 9 | 9 | 5 | 5 | 3 | 5 | 176,686 | 16,891 | B |
| Q20 actions-exec | 9 | 9 | 5 | 5 | 5 | 5 | 47,503 | 13,231 | B |

## Summary of all

| Metric | A (`gh`+RTK) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 8.55 | 9.00 |
| Research depth (mean) | 4.35 | 4.85 |
| Workflow (mean) | 4.40 | 4.95 |
| **Chars (total)** | **1,053,606** | **299,013** |
| Questions leaner | 7 | 13 |

B uses **71.6% fewer characters** overall and is higher on correctness (driven by Q13).

## Ground truth for the decisive question (Q13)

The prompt asks for the issue describing signed overflow in BITFIELD **`#<offset>` parsing**
and its fix PR. Verified by primary evidence:

- **Issue `#15389`** "[BUG] Redis BITFIELD `#offset` Signed Overflow DoS" — the `#<offset>`
  form multiplies the parsed offset by field width in signed `long long` before bounds checks.
- **Function `getBitOffsetFromArgument()`**, sink at **`src/bitops.c:730`** (`loffset *= bits`).
- **First overflowing i64 offset `#144115188075855872`** = `floor(LLONG_MAX/64)+1`; UBSan log:
  `bitops.c:730 … signed integer overflow: 64 * 144115188075855872 …`.
- **Merged PR `#15433`** "Fix signed overflow in BITFIELD #offset parsing" (merged into
  `unstable`), **2 files, +18/−3** — `src/bitops.c` (+9/−3), `tests/unit/bitfield.tcl` (+9/0).

**Arm B matched all of this exactly.** **Arm A answered a different bug** — issue `#15550`
("signed overflow *check* invokes UB", function `checkSignedBitfieldOverflow`, PR `#15545`,
+7/−2). That is a real but distinct SET/INCRBY-overflow issue, **not** the `#<offset>`
parsing overflow the question asks for. A's answer is confidently wrong, so it cannot win Q13
regardless of its lower char count.

## Per-question notes (short)

Both arms converged on the same primary evidence for **Q1–Q12 and Q14–Q20**; the two
independent research paths agreeing is the correctness signal, cross-checked against the
grader's own reads for the disputed/structured items.

- **Q2** — Both answer NO for `isQuantumSuperposition` with bounded-absence evidence. A is
  dramatically leaner (708 vs 27,502 chars): a single empty `gh search code` sufficed, whereas
  B additionally enumerated the export surface. Equal correctness; A leaner.
- **Q5** — Both name the runtime-core hydration `asyncDep` broadening and the runtime-vapor
  interop fix and justify changes in both packages. B enumerates a third concrete scenario
  (`vdomInterop` unmount stale-DOM) with file-level anchors → slightly deeper; A marginally
  thinner, hence Correctness A 8 / B 9.
- **Q13** — decisive divergence, resolved above.
- **Q14 / Q18** — structured dependency-section facts; both arms read the exact `package.json`
  and agree (Q14: `vite` in `peerDependencies` + `peerDependenciesMeta.optional:false` + `devDependencies`;
  Q18: `lightningcss` dep-only, `sass` dev+peer(optional:true), `fsevents` optionalDependencies).
- **Q11 / Q17 / Q19** — **fairness caveat:** arm A's very large char counts (228K / 256K / 176K)
  come from raw full-file `gh api …raw` reads of large source files. `gh` has **no region-targeted
  read**, so a full-file pull is arm A's leanest *exact-read* path here; octocode used targeted
  region/matchString reads. This is a structural arm-A constraint, not a runner error — but it is
  the main driver of the aggregate character gap.

## Bottom line

Not an equal-correctness tie: **Octocode (B) is both more correct (Q13) and 71.6% leaner** in
this pass. The efficiency gap is concentrated in multi-hop large-file reads (Q11/Q17/Q19) where
`gh`'s lack of region reads forces full-file pulls. Treat as one comparative snapshot; re-run for
a stable multi-pass claim.
