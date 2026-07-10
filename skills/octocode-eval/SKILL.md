---
name: octocode-eval
description: "Use when evaluating agent/skill/harness/code quality, defining goal→KPI cascades, benchmarking, running nested improve loops, error analysis, keep/discard experiments, held-out checks, or self-improvement without vibes."
---
# Octocode Eval
Teach agents to evaluate, link goals to KPIs, benchmark honestly, run nested improve loops, and self-improve with evidence — not vibes.
Flow: `ERROR-ANALYZE → FRAME(goal→KPI) → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE`.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.

## Hard Rules
- No goal→KPI link → STOP. No measurable primary → STOP.
- Narrative-only accept → REJECT. Editing harness/cases/graders to pass → REJECT.
- ACCEPT only if primary moves on held-out **and** guardrails hold.
- Prefer deterministic graders; binary/LLM next; humans calibrate. Grade outcomes over paths.
- Public benches orient; private failure suites gate ships. Distrust saturated/contaminated boards.
- Pick the right nested loop (experiment / suite / meta) — don’t conflate them.

## Workflow
1. **ERROR-ANALYZE** — open/axial-code traces → failure taxonomy (skip if taxonomy exists)
2. **FRAME** — goal → success → primary (lagging) → leading → guardrails → decision rule
3. **BASELINE** — measure under fixed budget; freeze harness for this experiment
4. **LOOP** — smallest subject change → measure → keep|discard (experiment loop)
5. **JUDGE** — graders; transcript fairness; capability vs regression; bench hygiene
6. **CAPTURE** — durable lesson + failureSignature; one owner
7. **VERIFY** — held-out + loop-report + skill/package checks
8. **SUITE-EVOLVE** — add cases from new failures between experiments (never to greenwash)
Stop when: goal/KPI undefined; checks not run; harness cheated; or another loop won’t change the verdict.

## Reference Map
- `references/error-analysis.md` — when deciding what to measure from real failures
- `references/goal-kpi-cascade.md` — when linking user goal to leading/lagging KPIs
- `references/kpi-contract.md` — when filling baseline/target/budget/held-out fields
- `references/nested-loops.md` — when choosing experiment vs suite vs meta loop
- `references/agent-loop.md` — when running keep/discard experiments
- `references/eval-techniques.md` — when choosing graders, pass@k, fail-to-pass
- `references/benchmarking.md` — when trusting or retiring public/private benches
- `references/eval-harness.md` — when adding `evals/cases.json` + `eval-*.mjs`
- `references/held-out-and-guards.md` — when preventing overfitting / contamination
- `references/karpathy-patterns.md` — when grounding in Software 2.0 / autoresearch / council
- `references/routing.md` — when handing off to Awareness / skills / research / RFC
- `references/improve-loop.md` — when closing a meta/harness improvement cycle
- `references/output.md` — when presenting the loop report

## Scripts
- `scripts/loop-report.mjs` — when validating a loop report has goal→KPI/baseline/result/verdict
- `scripts/eval-eval.mjs` — when self-testing answers against `evals/cases.json`

## Related skills
- `octocode-research` — evidence for claims under test · `octocode-skills` — folder edits after ACCEPT
- `octocode-awareness` — durable lessons / verify debt · `octocode-rfc-generator` — `KPI.md`
- `octocode-prompt-optimizer` — wording only (still run this KPI loop)
- `octocode-brainstorming` — worth-building before an eval suite
Install: `npx octocode skill --name octocode-eval`.
