---
name: octocode-graph-eval
description: "Use when measuring whether a change helped or looping until a target metric is hit — for one agent loop or a graph of loops (multi-agent workflow): ACCEPT/REVERT, keep/discard, goal→KPI contracts, eval suites, graders, held-out checks, benchmarks, TDD failing-case-first, don't-stop-till-done optimization (e.g. profiler-guided p95/memory targets), building the feedback loop/sensor before iterating, or node-level vs end-to-end evals (loop engineering / graph engineering)."
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

## Workflow
1. Error-analyze traces into a failure taxonomy; frame success, primary/leading metrics, guardrails, and decision rule.
2. Measure a fixed-budget baseline; make the smallest subject change; keep or discard from comparable results.
3. Judge grader quality, fairness, capability versus regression, and contamination; capture one durable lesson.
4. Verify held-out results and required checks; then add new failure cases between experiments.
Stop when goal/KPI is undefined, checks did not run, the harness changed to pass, or another loop cannot change the verdict.

## Smart routes — load only what the current step needs
- When deriving failures, load `references/error-analysis.md`; when connecting intent to measures load `references/goal-kpi-cascade.md`, then fill `references/kpi-contract.md` — make success and budget explicit.
- When choosing experiment, suite, or meta scope, load `references/nested-loops.md`; before the first iteration load `references/feedback-loops.md`, then for the inner keep/discard cycle load `references/agent-loop.md` — no workable sensor, no loop.
- When the subject is a multi-agent workflow (graph of loops), load `references/graph-of-loops.md` — primary KPI at the graph boundary, sensors per node, strengthen verifiers before adding nodes.
- When selecting graders or statistical checks, load `references/eval-techniques.md`; when trusting public/private suites load `references/benchmarking.md` — match evidence strength to the decision.
- When creating cases and runners, load `references/eval-harness.md`; before acceptance load `references/held-out-and-guards.md` — prevent leakage, overfitting, and greenwashing.
- When grounding methods in primary patterns, load `references/karpathy-patterns.md` — anchor techniques in proven loops.
- When a result needs another skill or durable capture, load `references/routing.md`; when closing a meta improvement cycle load `references/improve-loop.md` — transfer ownership without losing the decision rule.
- When reporting, load `references/output.md` and run `scripts/loop-report.mjs` — require goal, baseline, result, and verdict.

## Related routes and verification
- Use `octocode-research` for evidence under test; `octocode-brainstorming` before evaluating an unresolved idea; `octocode-rfc-generator` for a design KPI contract.
- Use `octocode-subagent` to fan out parallel hypotheses or benchmark trials within one iteration — measurement, keep/discard, and graders stay frozen here.
- Use `octocode-prompt-optimizer` for wording after the KPI is fixed; `octocode-skills` for folder edits after ACCEPT; `octocode-awareness` for durable lessons and verification debt.
- When changing this skill, run `scripts/eval-eval.mjs --self-test` and a matching `--case` — catch self-routing regressions.
- Sibling rich harness example: `octocode-orchestrator-local-worker` (`evals/kpi-contract.json` + live grades).
