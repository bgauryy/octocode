---
name: octocode-eval-benchmark
description: "Use when measuring whether a change helped: define KPIs, baselines, held-out cases, benchmarks, and keep/discard gates."
---
# Octocode eval benchmark
Design trustworthy evals and benchmarks, then run evidence-backed improvement loops for one agent or a multi-agent workflow.
Flow: `ERROR-ANALYZE → FRAME(goal→KPI) → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE`.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.

Workspace output contract: chat-only results stay in chat. New eval reports, frozen harness snapshots, and benchmark artifacts default to `<workspace>/.octocode/octocode-eval-benchmark/`; scratch runs use `<workspace>/.octocode/tmp/octocode-eval-benchmark/`. User-approved subject or suite edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Rules
- Link you goal to one measurable primary KPI, a runnable sensor, a fixed budget, guardrails, and a decision rule before iterating.
- Establish a failing case or below-target baseline before changing the subject. Keep the harness frozen during an experiment; grow the suite between experiments only.
- Accept only when comparable held-out results improve and guardrails hold. Never edit cases or graders to make a candidate pass.
- Prefer deterministic outcome graders; use calibrated model or human judgment where deterministic checks cannot capture quality.
- Public benchmarks orient; private failure suites gate releases. Account for contamination, saturation, and variance.
- For multi-agent workflows, verify real dependencies, fresh-context verification, counter-metric guardrails, and at least one deterministic anchor.

## Workflow
1. Error-analyze traces into a failure taxonomy; frame success, primary/leading metrics, guardrails, and decision rule.
2. Measure a fixed-budget baseline; make the smallest subject change; keep or discard from comparable results.
3. Judge grader quality, fairness, capability versus regression, and contamination; capture one durable lesson.
4. Verify held-out results and required checks; then add new failure cases between experiments.
Stop when the contract is undefined, checks cannot run comparably, the harness changed mid-experiment, or another iteration cannot change the verdict. <!-- style-lint: ignore-line passive-voice -->

## Smart routes — load only what the current step needs
- When deriving failures, load `references/error-analysis.md`; when connecting intent to measures load `references/goal-kpi-cascade.md`, then fill `references/kpi-contract.md` — make success and budget explicit.
- When choosing experiment, suite, or meta scope, load `references/nested-loops.md`; before the first iteration load `references/feedback-loops.md`, then for the inner keep/discard cycle load `references/agent-loop.md` — no workable sensor, no loop.
- When the subject is a multi-agent workflow (graph of loops), load `references/graph-of-loops.md` — run edge detection first, require anchor nodes, check verifier independence, name Goodhart guardrails, then set primary KPI at the graph boundary with per-node sensors.
- When auditing that graph for structural failure risk before trusting its green lights — shared context, opaque state, no checkpoint/resume, unbounded tool permissions, missing human gates — load `references/graph-failure-modes.md`; add a suite case on a mode's first trace appearance.
- When managing or measuring subagents under eval, load `references/subagent-cookbook.md` first for the ownership split; spawn mechanics stay in `octocode-subagent`.
- When running an evaluated multi-agent iteration, load `references/subagent-protocol.md` for the frozen FRAME→verdict protocol; when choosing worker and graph-boundary metrics, load `references/subagent-kpis.md` so spawn cost is measured, not invisible. <!-- style-lint: ignore-line passive-voice -->
- When defining how parent and workers talk during an evaluated run, load `references/subagent-communication.md` — bad channels create false certainty and unattributable failures; when choosing the topology itself, load `references/subagent-approaches.md` because the pattern decides which KPIs and checks matter.
- When inner loop is flat and no new hypothesis exists, suspect stuck search priors — load `references/nested-loops.md` for bilevel escalation, then `references/karpathy-patterns.md` for the Bilevel Autoresearch pattern.
- When selecting graders or statistical checks, load `references/eval-techniques.md`; when grading agent tool-call sequences or multi-turn trajectories load `references/trajectory-grading.md`; when trusting public/private suites load `references/benchmarking.md` — match evidence strength to the decision.
- When creating cases and runners, load `references/eval-harness.md`; before acceptance load `references/held-out-and-guards.md` — prevent leakage, overfitting, and greenwashing.
- When grounding methods in primary patterns, load `references/karpathy-patterns.md` — anchor techniques in proven loops.
- When a result needs another skill or durable capture, load `references/routing.md`; when closing a meta improvement cycle load `references/improve-loop.md` — transfer ownership without losing the decision rule.
- At SUITE-EVOLVE, add cases only from observed failures between experiments; use `references/error-analysis.md` and `references/eval-harness.md`.
- When reporting, load `references/output.md`, and run `scripts/loop-report.mjs` — require goal, baseline, result, and verdict.

## Related routes and verification
- Use `octocode-research` for evidence under test; `octocode-brainstorming` before evaluating an unresolved idea; `octocode-rfc-generator` for a design KPI contract.
- Use `octocode-subagent` to fan out parallel hypotheses or benchmark trials within one iteration — measurement, keep/discard, graders, and the subagent cookbook (`references/subagent-cookbook.md`) stay frozen here.
- Use `octocode-prompt-optimizer` for wording after the KPI is fixed; `octocode-skills` for folder edits after ACCEPT. <!-- style-lint: ignore-line passive-voice -->
- When changing this skill, run `scripts/check-description.mjs` then `scripts/eval-skill.mjs --self-test`, and a matching `--case` — catch trigger and self-routing regressions; cases live in `evals/` (`cases.json`, `trigger-cases.json`, `kpi-contract.json`).
