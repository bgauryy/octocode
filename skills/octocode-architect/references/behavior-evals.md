# Behavior Evaluations

Load when calibrating or changing the skill. Why: verify architectural rigor without making small tasks ceremonial.

## Contract

The executable cases live in `evals/cases.json`; `scripts/eval-skill.mjs` grades required and forbidden behavior. Cases cover:

| Behavior | Pass signal | Regression signal |
|---|---|---|
| Private rename | local scope and narrow verification | invented rollout or security work |
| Public contract | consumers, compatibility, migration, rollback | type-only edit |
| Similar helper | search, reuse, consolidate, or justify divergence | duplicate behavior |
| Safe review | `Major: none` when evidence supports it | fabricated finding |
| Unclear dependency | verified facts, missing contract, pause | guessed API |
| Prompt optimization | metric, baseline, one-variable rerun | intuition-only improvement |
| Blast radius | callers, data, runtime, tests, operations, rollback | file-list-only impact |
| Housekeeping | task-adjacent cleanup only | unrelated refactor |
| Bookkeeping | repository-required records updated | blanket metadata churn |
| Data/interface wiring | source, transform, boundary, sink, invariants | code treated as isolated text |
| Decomposition | dependencies, bounded parallel work, per-part verification, composition | indiscriminate workers or hidden failures |

## Run

```bash
node scripts/eval-skill.mjs --self-test
node scripts/eval-skill.mjs --case <id> --answer <response.md>
node scripts/eval-skill.mjs --batch <answer-directory>
```

`--self-test` proves every reference answer passes and every failure example fails. `--batch` expects `<id>.md` for each case and reports the aggregate pass rate.

Keep cases frozen during an instruction experiment. Accept only when the target failure improves, prior cases stay green, and `skill-review.mjs` reports zero errors. Add cases between experiments, never to excuse a failing candidate.

Next: return to `SKILL.md` and keep only evidence-backed revisions.
