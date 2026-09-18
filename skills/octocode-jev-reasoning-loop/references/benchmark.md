# Research-loop benchmark

Load when claiming the revised workflow improves decisions or calibrating policy thresholds. Why: schema validity does not measure recovery from a wrong initial belief.

## Frozen KPI

`evals/kpi-contract.json` owns the goal, primary semantic KPI, deterministic implementation gate, guardrails, and accept rule. Run `node scripts/eval-decision-loop.mjs` for the 15-case routing floor. Those cases test route and policy mechanics, not Jev judgment quality.

## Semantic variants

Compare with identical tasks, evidence access, retrieval budget, model pin, and call budget:

| Variant | Capability |
|---|---|
| A | Host agent only |
| B | Previous Jev workflow |
| C | + minimal DecisionBrief |
| D | + precommitted predictions and explicit outcomes on testable checks |
| E | + reflection-delta |
| F | + decision-review |
| G | Full workflow |

Primary semantic KPI: **Wrong-Lean Recovery Rate** — among tasks where the initial attractive lead is wrong and later evidence contradicts it, the fraction where the host abandons or materially reframes that lead before the final claim. Report final correctness separately: recovery is the decision-discipline mechanism, not a synonym for a lucky answer.

Also record final correctness, false conclusions, unsupported assertions, next-action quality, evidence retrievals, unnecessary calls, Jev calls, tokens, elapsed time, hypothesis-set reframes, and correct deterministic skips. Keep assertion rate and total calls as guardrails.

## Protocol

Freeze tasks, gold conclusions, source revisions, request packets, model version, thresholds, budgets, and grader before the run. Use at least the plan’s failure classes; split invention cases from held-out cases. Do not tune on held-out outcomes. Run enough repeated trials to expose variance and preserve raw requests, responses, APPLY records, precommitments, observed deltas, and source anchors under `<output>/octocode-jev-reasoning-loop/benchmark/`.

Accept a workflow change only when held-out Wrong-Lean Recovery Rate improves and false conclusions, unsupported assertions, total tool/token cost, and unnecessary Jev calls stay within frozen guardrails. A coherent typed answer is a leading indicator, not the final outcome. If the host applies correct judgments poorly, fix APPLY or composition rather than crediting the model.

Next: this step ends with an ACCEPT/REVERT verdict and preserved run artifacts.
