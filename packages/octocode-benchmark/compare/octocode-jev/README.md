# Octocode+Jev arm (`octojev`) — 30 GitHub questions, local CLI, Sonnet runner

The `octojev` arm answers the shared [GitHub questions](../github-questions/) (Q1–Q30)
using the full local Octocode surface through the native CLI
(`node packages/octocode/out/octocode.js tools <tool> --queries …`), **including the two
Jev tools** (`jevScout`, `jevReasoning`) and the `ask-file` driver. Historical
correctness for the `gh` and `octocode` arms lives in the three-pass reports under
[`../../results/`](../../results/README.md).

Canonical protocol files (this campaign follows them except where a deviation is
declared below):

- [`BENCHMARK.md`](../../skills/octocode-benchmark/references/BENCHMARK.md) — design + fairness rule
- [`RUNNER.md`](../../skills/octocode-benchmark/references/RUNNER.md) — runner contract, `## Q<n>` answer sections
- [`RUNNER_TOOL_CONTEXT.md`](../../skills/octocode-benchmark/references/RUNNER_TOOL_CONTEXT.md) → arm primer = [`primer-octocode.md`](../../skills/octocode-benchmark/references/primer-octocode.md) + [`primer-octocode-jev.md`](../../skills/octocode-benchmark/references/primer-octocode-jev.md)
- [`JUDGING.md`](../../skills/octocode-benchmark/references/JUDGING.md) — ground-truth-first, reasoning-first verdicts; unproven counts as wrong

The runner receives ONLY: the runner contract, its two arm primers, and the questions —
no question-specific advice, no worked examples on benchmark repositories, no answer key.
Judges receive ONLY the question and the answer section — tool identity hidden, no
transcript access; each judge establishes ground truth itself before reading the answer.

## Declared deviations from the canonical pairwise protocol

| Canonical | This campaign | Why |
|---|---|---|
| Fresh agent per (question, arm, pass) | One Sonnet session answers Q1–Q30 sequentially (canonical already permits batching questions within one arm's agent) | models a real agent working a task list; enables session-level token accounting |
| `compare/bin` instrumented wrappers, characters both directions | Big-model tokens from harness transcripts; Jev billed usage self-reported per question from tool output (cross-checkable in the transcript) | no wrapper exists for the local-CLI+Jev surface; token meters are the quantity of interest |
| Pairwise blind X/Y judging, 3 passes | Single-answer verdicts per `JUDGING.md` scoring (Correctness 0–10, Research depth 1–5, Workflow 1–5), single pass | one arm; directional evidence only — contested questions need the full pairwise treatment before strong claims |
| `npx octocode@<ver>` pinned | Local build (record `packages/octocode` version + repo SHA in the report) | the Jev tools ship in the local build |
| `ghCloneRepo` in surface | Excluded (auth defect: `Bearer` on git endpoints; `.octocode/GOTCHAS.md` 2026-09-19) | fails even on public repos until fixed |

**Do not pool** these numbers with the historical character measurements or Terra
metrics; historical correctness may be juxtaposed as clearly-labeled context only.

## Two arms — measuring the Jev delta

The `octojev-p1` run (jev **optional**, observed-state triggers) made **zero** Jev calls
— honest triggers keep Jev idle on targeted questions — so behaviorally it is the
**without-Jev baseline**. The delta arm exercises both Jev tools at fixed checkpoints
(per the jev-terra verdict: wire deterministic points, don't ask the host to self-judge):

| Arm | Jev protocol |
|---|---|
| `octojev` (baseline, = without-Jev behavior) | Tools available, observed-state triggers only → 0 calls in practice |
| `jevwired` | **Checkpoint A (scout):** any search/history result with 3+ candidate rows/files must be ranked by `jevScout` items-mode before fetching; fetch only `read`/`gray_read`-marked. **Checkpoint B (reasoning):** before writing each `## Q<n>` section, run `jevReasoning` route `hallucination_gate` on the section's core claim with the gathered evidence items; on `blocked`, retrieve what it names, revise, re-gate once. Both checkpoints record billed usage and outcomes per question |

Same questions, same runner model, same primers, same judging. The comparison isolates
what mandatory Jev participation adds or costs (correctness, fetch avoidance, blocked
claims, tokens, wall clock).

## Artifacts

| Artifact | Location |
|---|---|
| Runner answers (`## Q<n>` sections: Answer + Research steps + Jev usage line) | `answers/octojev-p1.md`, `answers/jevwired-p1.md` |
| Judge verdicts (reasoning-first, one file per question) | `judge/Q<n>.md` (baseline), `judge-jevwired/Q<n>.md` |
| Rollups | `RESULTS.md` (baseline), `RESULTS-COMPARISON.md` (delta) |
