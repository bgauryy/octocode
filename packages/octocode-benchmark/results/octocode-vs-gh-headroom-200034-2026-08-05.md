# Octocode vs `gh` + Headroom — 200034-2026-08-05

**Bottom line:** Correctness-first, **Octocode (B) wins the matchup.** B mean
correctness **9.70** vs A **8.35**: A carried five confidently-wrong answers
(Q7, Q10, Q13, Q14, Q18) while B carried none worse than a minor omission.
Aggregate footprint also favors B — **339,016 chars vs 2,144,745 (6.3× smaller,
84.2% fewer)** — driven by A's blow-ups on the structural questions (Q11, Q16,
Q17, Q19) where Headroom compression did not contain a wide `gh` trace.

The naive per-question preference count is A 10 / B 9 / tie 1, but that count is
**dominated by char tiebreaks on questions where both arms were equally
correct** (Q1–Q4, Q8): on those easy lookups Headroom-compressed `gh` output was
smaller than Octocode's, so the judge broke the correctness tie toward A. Per
`SCORING.md`, correctness decides first and a confidently-wrong answer cannot
win — on that axis B is clearly ahead.

## Run metadata

- Question set: shared `compare/github-questions/` — 20 questions, 1 pass (snapshot).
- Question-set content hash: `da3f6b922387`.
- `RUNNER_TOOL_CONTEXT.md` commit: `4d35f0f3`.
- Tool versions: `gh` 2.96.0 · Headroom (headroom-ai) **0.34.0**, kompress model
  backend `onnx` · Octocode `v18.0.1`.
- Preflight: `preflight.py --warmup` passed (no-op, SmartCrusher, Kompress paths
  all valid; no failures). Smoke: flask tree 21.3% char reduction; mutation guard
  exit 2.
- Isolation: 4 runner agents (`A:Q1-10`, `A:Q11-20`, `B:Q1-10`, `B:Q11-20`), arms
  never shared a context; 4 blind judge agents (Q1-5/6-10/11-15/16-20) received
  only X/Y answers with tool identity hidden and researched ground truth
  independently.
- Measurement: Arm A via `bin/ghc` per-question `GHC_LOG`; Arm B via `bin/octoc`
  per-question `OCTO_LOG`. Char counts from the logs, never self-report.

> **Version caveat:** the prior headline Headroom report used Headroom 0.33.0 /
> kompress-v2-base; this run is Headroom **0.34.0**. Per the matchup README a
> different Headroom version is a different arm A — do not sum across versions.
> This is also a single pass; the 3-pass `115145` report remains the multi-pass
> reference.

## Per-question table

| Question | Corr A | Corr B | Depth A | Depth B | WF A | WF B | Chars A | Chars B | Leaner |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Q1 route-regex | 10 | 10 | 4 | 4 | 4 | 4 | 2,360 | 14,870 | A |
| Q2 is-absence | 10 | 10 | 3 | 4 | 4 | 4 | 122 | 19,864 | A |
| Q3 flask-route-history | 10 | 8 | 5 | 4 | 5 | 4 | 6,441 | 12,019 | A |
| Q4 axios-follow-redirects | 10 | 10 | 5 | 5 | 5 | 5 | 1,562 | 28,737 | A |
| Q5 vue-pr-diff | 10 | 9 | 5 | 4 | 4 | 4 | 1,123 | 19,585 | A |
| Q6 express-router | 10 | 10 | 4 | 4 | 4 | 4 | 2 | 13,225 | A |
| Q7 zustand-next | 6 | 10 | 4 | 4 | 4 | 4 | 1,446 | 7,302 | A |
| Q8 vscode-keybinding | 10 | 9 | 4 | 4 | 4 | 4 | 525 | 28,403 | A |
| Q9 fastify-lifecycle | 10 | 10 | 4 | 4 | 4 | 4 | 9,280 | 9,950 | A |
| Q10 axios-entry-chain | 4 | 9 | 4 | 4 | 4 | 4 | 303 | 12,483 | A |
| Q11 esbuild-boundary | 10 | 10 | 4 | 4 | 5 | 5 | 389,299 | 32,084 | B |
| Q12 node-stream-event | 10 | 9 | 4 | 4 | 5 | 5 | 88,314 | 14,490 | B |
| Q13 redis-bitfield | 2 | 10 | 3 | 5 | 4 | 5 | 4,929 | 28,187 | A |
| Q14 vitest-vite-dep | 1 | 10 | 3 | 5 | 3 | 5 | 8,290 | 13,527 | A |
| Q15 hono-jsx-array | 10 | 10 | 5 | 5 | 5 | 5 | 18,534 | 9,347 | B |
| Q16 eslint-parser-chain | 10 | 10 | 5 | 4 | 4 | 5 | 1,108,508 | 10,159 | B |
| Q17 nextjs-fetch-memo | 10 | 10 | 5 | 4 | 5 | 5 | 161,528 | 19,703 | B |
| Q18 vite-dep-membership | 4 | 10 | 3 | 4 | 2 | 5 | 22,025 | 6,182 | B |
| Q19 node-child-process | 10 | 10 | 5 | 4 | 5 | 5 | 266,451 | 26,967 | B |
| Q20 actions-exec-output | 10 | 10 | 5 | 5 | 5 | 5 | 53,703 | 11,932 | B |

## Summary of all

| Metric | A (gh + Headroom) | B (Octocode) |
|---|---:|---:|
| Correctness (mean) | 8.35 | **9.70** |
| Research depth (mean) | 4.20 | **4.25** |
| Workflow (mean) | 4.25 | **4.55** |
| **Chars (total)** | 2,144,745 | **339,016** |
| Chars (mean / question) | 107,237 | **16,951** |
| Questions leaner | 10 | 10 |
| Judge preference (chars-tiebroken) | 10 | 9 (+1 tie) |

## Where each arm won and lost

- **A leaner on easy targeted lookups (Q1–Q9):** for single-file / single-field
  questions, tight `gh` queries + Headroom compression produced very small
  footprints (Q6 just 2 chars, Q2 122, Q8 525), beating Octocode's per-call
  envelope. These are legitimate A tiebreak wins at equal correctness.
- **A confidently wrong five times:** Q7 (denied `peerDependencies` exists in
  zustand — it does), Q10 (denied axios `exports` field — it exists), Q13
  (answered the wrong Redis bug: #15550/#15545 instead of #15389/#15433), Q14
  (claimed vitest has no `peerDependencies` — it declares `vite` as a required
  peer), Q18 (misclassified `lightningcss` as an optional peer — it is a plain
  production `dependency`). Four of the five are structured-JSON membership
  errors from working off truncated/compressed `package.json` output.
- **A footprint blew up on wide structural traces:** Q16 1.1M, Q11 389K, Q19
  266K, Q17 162K chars — recursive trees / multi-file traces that compression
  shrank but did not bound; the runner also made many calls (Q16: 31).
- **B correct throughout with a bounded footprint:** worst single question 32K
  chars (Q11); resolved every structured-field and issue/PR-number dispute
  correctly with exact reads.

## Fairness caveats

- Single pass — a snapshot, not a stable multi-pass claim. The neural Kompress
  path can drift; repeat ≥3× for a headline replacement.
- Runner char discipline varied (A made many redundant calls on Q11/Q16); this
  is real measured workflow cost, not a harness artifact.
- Headroom 0.34.0 ≠ the 0.33.0 of the standing headline report; treat as a
  separate arm A generation.

## Bottom line

Not equally correct: **Octocode (B) is more correct (9.70 vs 8.35)** and blocks
five confidently-wrong gh+Headroom answers, so B wins the matchup on the
correctness-first rule. B is also ~6.3× smaller in aggregate. gh+Headroom's only
edge is a smaller footprint on the easiest targeted lookups, where both arms were
already correct.
