# Octocode Eval Benchmark

Design trustworthy evaluations and benchmarks that decide whether a code, prompt, skill, agent, or multi-agent workflow improved.

## Use when

- A change needs a measurable keep-or-revert decision, not just passing tests.
- You need goals, KPIs, baselines, guardrails, graders, or held-out cases.
- Tests pass but don’t establish the behavior or quality outcome you care about.

## Not for

- Ordinary ship checks where tests passing is enough → just run the tests
- Investigating a code claim without a measurement goal → `octocode-research`
- Writing or repairing documentation → `octocode-documentation`

## Workflow

```text
ERROR-ANALYZE → FRAME → BASELINE → LOOP → JUDGE → CAPTURE → VERIFY → SUITE-EVOLVE
```

Do not edit a grader or case to make a candidate pass. Freeze sensors before measuring.

## Install

```bash
npx -y octocode skill install octocode-eval-benchmark
```

## Maintainer verification

```bash
node scripts/loop-report.mjs --self-test
node scripts/eval-skill.mjs --self-test
node scripts/check-description.mjs
```

Then run the `octocode-skills` review against this folder.
