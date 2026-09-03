# Octocode eval benchmark

Design trustworthy evaluations and benchmarks that decide whether a code, prompt, skill, agent, or multi-agent workflow improved.

## Use when

- A change needs a measurable keep-or-revert decision.
- You need goals, KPIs, baselines, guardrails, graders, or held-out cases.
- Tests pass but do not establish the behavior or quality outcome.

## Capabilities

- Turns error analysis into failure taxonomies and evaluation cases.
- Cascades goals into primary metrics, leading indicators, guardrails, and decision rules.
- Combines deterministic checks, binary questions, model judges, and human review.
- Separates training, regression, and held-out evidence.
- Measures multi-agent boundaries, per-node sensors, verifier independence, cost, and collisions.
- Uses failing-case-first loops and rejects changes that do not improve the frozen sensor.

## Workflow

```text
ERROR-ANALYZE → FRAME → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE
```

Do not edit a grader or case merely to make a candidate pass.

## Install

```bash
npx octocode skill install octocode-eval-benchmark --platform codex
```

## Maintainer verification

```bash
node scripts/loop-report.mjs --self-test
node scripts/eval-skill.mjs --self-test
node scripts/check-description.mjs
```

Then run the `octocode-skills` review against this folder.
