---
name: octocode-graph-eval
description: "Use when you need a measurable keep/discard loop — goal→KPI, baseline vs target, held-out checks, eval suites, or don't-stop-till-done against a runnable sensor. Not for ordinary ship checks where 'tests passed' is enough."
---
# Octocode Graph Eval
Evaluate outcomes and run improvement loops with evidence, not vibes — for one loop or a graph of loops.
Flow: `ERROR-ANALYZE → FRAME(goal→KPI) → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE`.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.

## Lobby rules
- No goal→KPI link → STOP. No measurable primary → STOP. No runnable sensor → build one before looping.
- Narrative-only accept → REJECT. Editing harness/cases/graders to pass → REJECT.
- ACCEPT only if primary moves on held-out **and** guardrails hold.
- Prefer deterministic graders; binary/LLM next; humans calibrate. Grade outcomes over paths.
- **TDD for agents:** write or select a failing case / KPI check **before** mutating the subject; green only after the change (red → green → keep|discard).
- Public benches orient; private failure suites gate ships. Distrust saturated/contaminated boards.
- Freeze the harness during an experiment; evolve the suite only between experiments.
- **Graph check:** before evaluating a multi-agent workflow, run edge detection — if no two nodes are independent (every step reads the prior step's output), it is a loop, not a graph. Build a loop.
- **Goodhart guard:** every primary KPI must have a counter-metric guardrail the agent cannot tune. Primary improving + guardrail degrading → reframe the goal, not the loop.
- **Verifier independence:** a verifier sharing the executor's context is not independent. Require fresh context before calling a result verified.
- **Anchor requirement:** every graph must have at least one node whose output cannot be argued with (tests that ran, build exit codes, type errors). No anchors → build one before trusting the graph.

## Workflow
1. Error-analyze traces into a failure taxonomy; frame success, primary/leading metrics, guardrails, and decision rule.
2. Measure a fixed-budget baseline; make the smallest subject change; keep or discard from comparable results.
3. Judge grader quality, fairness, capability versus regression, and contamination; capture one durable lesson.
4. Verify held-out results and required checks; then add new failure cases between experiments.
Stop when goal/KPI is undefined, checks did not run, the harness changed to pass, or another loop cannot change the verdict.

## Smart routes — load only what the current step needs
- When deriving failures, load `references/error-analysis.md`; when connecting intent to measures load `references/goal-kpi-cascade.md`, then fill `references/kpi-contract.md` — make success and budget explicit.
- When choosing experiment, suite, or meta scope, load `references/nested-loops.md`; before the first iteration load `references/feedback-loops.md`, then for the inner keep/discard cycle load `references/agent-loop.md` — no workable sensor, no loop.
- When the subject is a multi-agent workflow (graph of loops), load `references/graph-of-loops.md` — run edge detection first, require anchor nodes, check verifier independence, name Goodhart guardrails, then set primary KPI at the graph boundary with per-node sensors.
- When auditing that graph for structural failure risk before trusting its green lights — shared context, opaque state, no checkpoint/resume, unbounded tool permissions, missing human gates — load `references/graph-failure-modes.md`; add a suite case on a mode's first trace appearance.
- When managing or measuring subagents under eval, load `references/subagent-cookbook.md` first for the ownership split; spawn mechanics stay in `octocode-subagent`.
- When running an evaluated multi-agent iteration, load `references/subagent-protocol.md` for the frozen FRAME→verdict protocol; when choosing worker and graph-boundary metrics, load `references/subagent-kpis.md` so spawn cost is measured, not invisible.
- When defining how parent and workers talk during an evaluated run, load `references/subagent-communication.md` — bad channels create false certainty and unattributable failures; when choosing the topology itself, load `references/subagent-approaches.md` because the pattern decides which KPIs and checks matter.
- When inner loop is flat and no new hypothesis exists, suspect stuck search priors — load `references/nested-loops.md` for bilevel escalation, then `references/karpathy-patterns.md` for the Bilevel Autoresearch pattern.
- When selecting graders or statistical checks, load `references/eval-techniques.md`; when grading agent tool-call sequences or multi-turn trajectories load `references/trajectory-grading.md`; when trusting public/private suites load `references/benchmarking.md` — match evidence strength to the decision.
- When creating cases and runners, load `references/eval-harness.md`; before acceptance load `references/held-out-and-guards.md` — prevent leakage, overfitting, and greenwashing.
- When grounding methods in primary patterns, load `references/karpathy-patterns.md` — anchor techniques in proven loops.
- When a result needs another skill or durable capture, load `references/routing.md`; when closing a meta improvement cycle load `references/improve-loop.md` — transfer ownership without losing the decision rule.
- When reporting, load `references/output.md` and run `scripts/loop-report.mjs` — require goal, baseline, result, and verdict.

## Related routes and verification
- Use `octocode-research` for evidence under test; `octocode-brainstorming` before evaluating an unresolved idea; `octocode-rfc-generator` for a design KPI contract.
- Use `octocode-subagent` to fan out parallel hypotheses or benchmark trials within one iteration — measurement, keep/discard, graders, and the subagent cookbook (`references/subagent-cookbook.md`) stay frozen here.
- Use `octocode-prompt-optimizer` for wording after the KPI is fixed; `octocode-skills` for folder edits after ACCEPT.
- When changing this skill, run `scripts/check-description.mjs` then `scripts/eval-eval.mjs --self-test` and a matching `--case` — catch trigger and self-routing regressions; cases live in `evals/` (`cases.json`, `trigger-cases.json`, `kpi-contract.json`).
