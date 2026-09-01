# Octocode eval benchmark

`octocode-eval-benchmark` designs trustworthy evals and benchmarks, cascades goals into KPIs, and runs nested improvement loops with evidence instead of vibes. It evaluates a single agent, a code or prompt change, or a multi-agent workflow. For repository dependency topology, use `octocode-code-graph`.

## Who it’s for

- **Users / agents:** measurable bars before accepting skill, harness, or code changes
- **Maintainers:** adding cases/graders, failure taxonomies, or improve-loops for other Octocode skills

## Capabilities

- Error analysis → failure taxonomy → eval cases (not vanity metrics)
- Goal → success → lagging primary → leading drivers → guardrails → decision rule
- Nested loops: experiment (keep/discard) · suite (grow cases) · meta (harness/skill)
- Grader mix: deterministic, BinEval binary questions, LLM judges, humans, council
- Benchmarking hygiene: private ship gates; public boards orient; contamination/saturation checks
- Coding: fail-to-pass + pass-to-pass; outcomes over brittle paths
- TDD for agents: red (failing case) → green (same harness) → keep|discard
- Feedback-loop prerequisites: runnable sensor + numeric target + budget before any loop starts
- Graph of loops: end-to-end primary at the graph boundary, per-node sensors, attribution by bisection, strengthen verifiers before adding nodes
- Multi-agent eval gates: edge detection (real vs fake dependencies), verifier independence (fresh context required), Goodhart guard (counter-metric per KPI), anchor requirement (tests that actually ran)
- Multi-agent measurement: graph-boundary KPIs, per-node sensors, barriers, verifier independence, anchors, and collision/cost guardrails
- Bilevel escalation: when inner loop is flat with no new hypotheses, outer loop rewrites the search strategy — not just tunes program.md
- Scripts: `loop-report.mjs`, `eval-skill.mjs`, `check-description.mjs`

## Workflow

```text
ERROR-ANALYZE → FRAME(goal→KPI) → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE
```

## Installation

```bash
npx octocode skill --name octocode-eval-benchmark
npx octocode skill --add --path skills/octocode-eval-benchmark --platform common --force
```

## Scripts

```bash
node scripts/loop-report.mjs --self-test
node scripts/eval-skill.mjs --self-test
node scripts/check-description.mjs
```

Before shipping a change, run the skill review from the `octocode-skills` skill against this folder and clear every ERROR — this folder installs on its own, so it never hard-codes a path into a sibling.
