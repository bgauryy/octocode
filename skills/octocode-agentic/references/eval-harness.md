Load when building a runner, laying out a suite or run folder, identifying runs, or writing eval task text.

# Eval harness

## Runner

- Record grader evidence, source and version identities; no empty logs for unavailable evidence.
- Repairs get real operational errors, never answer hints; each is an extra, separately labeled attempt under the frozen policy.
- Verify what reached the model: no silent truncation of the rendered input (requested options and token estimates prove nothing).
- Script only recurring deterministic work; no framework for folder names or report prose.

## Suite and run folders

Reuse the project's runner and its native result files, run handles and version records. Keep tasks and permitted inputs separate from evaluator-only references, rubrics and graders. Document real setup/run/analysis commands; a starter without a runner says so. Worker-visible labels must not reveal answers.

Keep development/validation evidence separate from sealed-test evidence at the [saved-result path](../output.md#saved-result). Link large artifacts and native logs instead of copying them or imposing a second directory/schema.

## Run identity

Preserve enough to associate each result with its task, baseline/candidate, attempt, actual inputs, outputs, grading evidence and relevant conditions. Create identifiers or hashes only when needed to disambiguate attempts, detect replacement or reproduce a claim. Keep original attempts and grades; do not overwrite them with retries or reassessments.

## Adapt a suite

Keep only pieces that affect execution or interpretation. Include/exclude cases for recorded reasons; resolve material reviewer disagreement or mark the case ambiguous. Match task and reference records without exposing outcomes to the solver.

## Task text

A [task contract](protocol-catalog.md#catalog) that states every requirement the grader enforces, including a required output location, API, or business constraint; the solver picks the method, so never bug locations, rubric checklists, expected answers, or a preferred tool sequence.
