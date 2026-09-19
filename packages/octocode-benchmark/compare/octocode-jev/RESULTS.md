# Results — octojev arm, single Sonnet session, Q1–Q30 (2026-09-19)

Run of record: `answers/octojev-p1.md` (30 canonical `## Q<n>` sections; runner = Claude
Sonnet, one continuous session; local build `octocode 19.2.0` @ repo `5422539e`,
branch `codex/preproduction-hardening`). Verdicts: `judge/Q<n>.md`, one
ground-truth-first judge per question per `JUDGING.md` (single-answer deviation
declared in `README.md`). One judge (Q9) was re-run after a transient harness
classifier block; its verdict is from the identical prompt on retry.

## Headline

| Metric | Value |
|---|---|
| **Correctness** | **279/300 (93.0%)** — 18 questions at 10/10, 9 at 9, 2 at 8, 1 at 2 |
| Research depth | 141/150 |
| Workflow (leanest-path) | 146/150 |
| Runner tool calls | ~90 CLI invocations (self-reported; 177 harness tool uses incl. file writes) |
| **Jev calls** | **0 across all 30 questions** (0 input / 0 output Jev tokens) |
| Runner session tokens | 342 API turns · **122,228 output** · **749,515 fresh input** · 62,872,052 cache-read |
| Wall clock | 24.7 min (single sequential session) |

## The Jev finding — zero calls, and that is the correct result

Every section's Jev-usage line explains why no trigger fired: searches returned small
candidate sets whose inline snippets already constituted the read, or deterministic
evidence (exact cross-references, timestamps, exact-match lookups) settled the
question — and the primer's rule is that deterministic evidence outranks probabilistic
judgment. A handful of questions had 4+ result rows, but the rows' snippets resolved
them without additional reads to rank.

Interpretation, consistent with the frozen scout doctrine and the 2026-09-19 A/B
(`docs/JEV_BENCHMARK.md`): **jevScout pays when avoided reads are expensive** (0.19–0.40×
vs read-everything on suites built around expensive fan-outs). This question set, taken
with a capable runner on the leanest path, almost never creates that condition — the
observed-state triggers correctly kept Jev idle instead of adding ~150ms + tokens per
question. The arm's Jev value here was **optionality at zero spend**, not savings.
A question set with genuine wide fan-outs (many-candidate localization, large-diff
triage) is where the jev delta should be measured next.

## Session-token signature (single-session deviation)

The single continuous session re-reads its accumulated context every turn: 62.9M
cache-read tokens against only 0.75M fresh input — the cost signature of one long
session vs the historical fresh-agent-per-question isolation. Cache reads are billed at
a fraction of fresh input, but this is the honest trade of the "same agent answers all
30" design and is not comparable to per-question-isolated runs.

## Failures and errors

- **Q14 (correctness 2)** — the run's one real failure: misattributed a `vite` entry in
  `packages/vitest/package.json` `devDependencies` to `dependencies` (headline "both
  regular and peer" is wrong; peer-only, non-optional). Root cause: JSON section
  membership inferred across an elided matchString view — the exact failure class
  `JUDGING.md` warns about ("never infer membership across elided/minified
  boundaries"). The same mistake occurred independently in a discarded pilot run,
  indicating a systematic tool-guidance gap, now logged in `.octocode/GOTCHAS.md`.
- Minor deductions (9s/8s): a wrong path echo (Q4), a missing direct-file
  absence check (Q2), and similar single-fact gaps — see `judge/Q<n>.md`.

## Runner-reported product frictions (logged in `.octocode/GOTCHAS.md`)

1. `ghGetHistoryItem operation:"commit"` needs explicit `includeDiff:true` for hunks.
2. `ghGetFileContent` small-window reads can return a false-negative
   "Incomplete read" error although the window contains the needed content.
3. (From judging) matchString views must not establish JSON section membership.

## Historical context (different protocol/meter — juxtaposition only, never pooled)

The historical three-pass pairwise campaign (character-measured, fresh agent per
question, blind X/Y judges) put octocode at mean correctness 9.19/10 vs plain gh
9.27/10 with octocode 1.99× leaner (geo-mean chars). This run's 9.30/10 mean under a
different judge protocol is directionally consistent with "quality parity at strong
efficiency"; it does not update the pairwise character claims.

## Per-question verdicts

| Q | Correctness /10 | Depth /5 | Workflow /5 | Key errors |
|---|---|---|---|---|
| Q1 | 10 | 5 | 5 | none |
| Q2 | 9 | 4 | 5 | none |
| Q3 | 10 | 5 | 5 | none |
| Q4 | 9 | 5 | 4 | Answer body names the adapter path as "lib/axios/lib/adapters/http.js" — correct path is lib/adapters/http.js … |
| Q5 | 9 | 5 | 5 | Claim that the old queuePostFlushCb deferral was "causing anchor/child mismatches" inverts the removed comment… |
| Q6 | 10 | 5 | 5 | none |
| Q7 | 10 | 5 | 5 | none |
| Q8 | 10 | 5 | 5 | none |
| Q9 | 10 | 5 | 5 | none |
| Q10 | 9 | 4 | 5 | none |
| Q11 | 10 | 5 | 5 | none |
| Q12 | 10 | 4 | 5 | none |
| Q13 | 10 | 5 | 5 | none |
| Q14 | 2 | 2 | 2 | Claims vite is in "dependencies" (it is not — the entry is in devDependencies); claims vite "does not appear i… |
| Q15 | 9 | 5 | 5 | Claim that all remaining changed files besides base.ts and middleware/jsx-renderer/index.ts are test specs is … |
| Q16 | 10 | 5 | 5 | none |
| Q17 | 10 | 5 | 5 | none |
| Q18 | 10 | 5 | 5 | none |
| Q19 | 8 | 4 | 5 | none |
| Q20 | 10 | 5 | 5 | none |
| Q21 | 10 | 5 | 5 | none |
| Q22 | 10 | 5 | 5 | none |
| Q23 | 9 | 5 | 5 | Line citations off by 1-2 for ksys_write (727 vs 728), CLASS(fd_pos) (729 vs 730), and the vfs_write call (737… |
| Q24 | 10 | 5 | 5 | none |
| Q25 | 9 | 5 | 5 | Research steps claim the README.md selected patch contained "3 hunks" but the patch has 4 @@ hunks; minor over… |
| Q26 | 10 | 5 | 5 | none |
| Q27 | 10 | 4 | 5 | none |
| Q28 | 9 | 5 | 5 | "compaction triggers once usage crosses 75% of the token threshold" is circular/imprecise (threshold = 75% of … |
| Q29 | 9 | 5 | 5 | "explicitly including streamable HTTP" — the cited authorization file never names streamable HTTP, only "HTTP-… |
| Q30 | 8 | 4 | 5 | "How a message pipe is established" omits the concrete mechanism (BindNewPipeAndPassReceiver / PendingReceiver… |
