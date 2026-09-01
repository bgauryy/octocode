# Behavior Evaluations

Load when calibrating the skill or checking a revision. Why: verify that its rigor stays proportional and evidence-based.

## Scenarios

| Scenario | Expected behavior | Failure signal |
|---|---|---|
| Rename a private local variable | Confirm local scope, patch, run the narrow check, and report briefly | Invents rollout, security, or observability work |
| Change a public request field | Trace callers and consumers; name compatibility, validation, migration, tests, and rollback | Edits the type without checking runtime consumers |
| Add behavior resembling an existing helper | Search for similar implementations and contracts; reuse, consolidate, or justify intentional divergence | Adds another copy after inspecting only the first matching file |
| Review a safe mechanical diff | Return `Major: none` when evidence supports it | Fabricates a finding to fill the template |
| Dependency contract is unclear | Present verified facts, name the missing contract, and pause before implementation | Guesses the API or builds an abstraction around uncertainty |
| Optimize an agent prompt | Define a metric and baseline, change one variable, rerun, and compare | Claims improvement from intuition alone |

## Pass contract

For each scenario, check:

- The response follows `THINK → PLAN → CODE → REVIEW` without turning those labels into ceremony.
- Claims cite inspected code, runtime behavior, tests, or an explicitly named assumption.
- The plan names the smallest useful slice and excludes unrelated work.
- The response checks similar implementations and explains reuse, consolidation, or intentional divergence.
- Risk dimensions are addressed only when material; irrelevant dimensions are omitted or marked N/A with a reason.
- Verification exercises the production path or explains why that is impossible.
- The output uses the planning or review contract and stays concise.

A revision passes when every scenario avoids its failure signal and satisfies all applicable checks. Record any failed scenario before changing the instructions; change one instruction at a time, then rerun the full set.

This evaluation step ends here. Return to `SKILL.md` and keep only revisions that improve the failed scenario without regressing the others.
