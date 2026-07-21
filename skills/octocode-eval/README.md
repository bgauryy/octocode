# Octocode Eval

`octocode-eval` teaches agents how to evaluate work, cascade goals into KPIs, benchmark honestly, run nested improve loops, and self-improve with evidence instead of vibes.

## Who It’s For

- **Users / agents:** measurable bars before accepting skill, harness, or code changes
- **Maintainers:** adding cases/graders, failure taxonomies, or improve-loops for other Octocode skills

## The Problem

Agents claim “better” without baseline, held-out, or keep/discard. Public benches get contaminated or saturated. Holistic scores hide *why*. Orphan KPIs optimize the wrong goal. Without nested loops, people edit graders to greenwash flat experiments.

## Features / Capabilities

- Error analysis → failure taxonomy → eval cases (not vanity metrics)
- Goal → success → lagging primary → leading drivers → guardrails → decision rule
- Nested loops: experiment (keep/discard) · suite (grow cases) · meta (harness/skill)
- Grader mix: deterministic, BinEval binary questions, LLM judges, humans, council
- Benchmarking hygiene: private ship gates; public boards orient; contamination/saturation checks
- Coding: fail-to-pass + pass-to-pass; outcomes over brittle paths
- TDD for agents: red (failing case) → green (same harness) → keep|discard
- Scripts: `loop-report.mjs`, `eval-eval.mjs`

## Operating Model

```text
ERROR-ANALYZE → FRAME(goal→KPI) → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE
```

Grounded in Karpathy (Software 2.0, RLVR, autoresearch), Anthropic agent evals, BinEval, Hamel error analysis, and Octocode’s `Agent = Model + Harness` thesis.

## Installation

```bash
npx octocode skill --name octocode-eval
npx octocode skill --add --path skills/octocode-eval --platform common --force
```

## Scripts

```bash
node scripts/loop-report.mjs --self-test
node scripts/eval-eval.mjs --self-test
node ../../octocode-skills/scripts/skill-review.mjs .
```
