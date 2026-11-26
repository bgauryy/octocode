Load when iterating on a change with Keep or Discard, or picking the loop level to work at.

# Eval loop

## Keep/Discard loop

PDCA against the baseline; Keep or Discard is its act step.

```mermaid
flowchart TD
  B["Plan: record baseline copy, authority, relevant comparison conditions, comparable budget, and a failing case or below-target outcome"] --> H
  H["Plan: hypothesis test with mechanism, predicted effect, trigger, strongest rival, counterexample, result (confirmed, dismissed, unresolved); no case-specific answers or IDs in instructions"] --> D
  D["Do: smallest change to one coherent factor"] --> C
  C["Check: compare under the frozen plan"] --> A
  A{"Act: selection rule and guardrails hold?"}
  A -- yes --> K[Keep]
  A -- no --> X["Discard the targeted changes, never unrelated edits"]
  K --> S
  X --> S
  S{"Stop condition: budget, target, or hypotheses run out?"}
  S -- no --> H
  S -- yes --> E[Stop]
```

- Keep outcomes and evidence for the comparison, including failures and retry cost.

- Parallel hypotheses: separate subject copies and trials; never report the luckiest attempt as pass@1.
- Noise hides the effect: improve measurement first; cheap proxies may guide development, never final verification.
- Check valid alternatives, neighboring edge cases, old successes; a higher score alone proves nothing.

## Loop levels

- **Experiment (inner)**: developer or optimizer mutates one file, prompt, or skill paragraph; solvers get task inputs only. KPI: primary metric.
- **Suite (middle)**: error-analyze traces; add or fix cases, criteria, failure categories; rebalance. KPI: top-failure coverage, regression stay-green rate.
- **Meta (outer)**: authorized maintainer improves program, skill, graders, budgets, harness; new baseline and sealed final check; development traces only. KPI: fewer repeated failure signatures.
- Before escalating, check development failures for sensor defects or missing coverage. Grow the suite only between experiments, from real failures.
- Inner loop flat: missing cases go to suite level, a bad program to meta; with no new hypothesis or failure mode, mine traces for recurrent search patterns and change the search strategy in code or program, not only optimizer-instruction wording (`instructions/`).
