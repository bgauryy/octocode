# Eval harness
Load when implementing or extending a benchmark runner. Why: reliable execution and records make scores interpretable.

Use `benchmarks/README.md` for the common definition and per-run results layout. `benchmarks/document-answering/README.md` is the public task/judge starter; `benchmarks/skill-smoke/README.md` describes existing maintenance checks.

## Cases and access
Keep task IDs, provenance, family/split metadata and evaluator criteria in the controller. Export only the public question, relevant requirements and permitted inputs to the solver. Keep expected answers, private tests, rubric questions and failure-analysis notes evaluator-only. A framework's metadata can reach environment variables or tools; inspect its real export (`clean-lab.md`).

Build cases from representative tasks and observed failures. Check solvability, valid alternative solutions and deliberately wrong outputs before trusting the grader. Resolve material expert disagreements or mark ambiguity; do not require a ritual number of reviewers for every task.

Prefer executable checks for objective properties, calibrated model/human judgment for subjective ones. Regexes detect wording, not semantic correctness. Version task/fixture/grader changes and retain original outcomes; never change the harness mid-comparison to make the subject pass.

## Runner behavior
- Create a unique results directory before execution. Record effective settings/identities, actual exported inputs and inclusion/exclusion decisions. Do not create empty logs for unavailable evidence.
- Start independent trials with reset state and the intended production tools. Freeze available schemas, permissions, runtime/model versions and budgets for the comparison. Preserve legitimate within-trial interactions.
- Grade tool scope, semantic arguments, schema/transport validity and completion separately when relevant. A familiar tool name is not proof of a valid call.
- Distinguish first attempts, repairs, solver failures, judge errors, infrastructure errors and Unknowns. Give repairs actual operational errors, not answer hints; record them as additional attempts under the frozen policy.
- Confirm the actual rendered input fits the host context. Disable silent truncation where possible; otherwise retain evidence of what reached the model. Requested options or raw token estimates alone do not prove prompt fidelity.
- Seal outputs and final state before grading. Store grader evidence and sufficient source/version identities to reproduce the result. Missing records must remain visible in coverage and denominators.

Native runner logs can replace the suggested JSON files if they preserve these properties. Add scripts only for recurring deterministic work; do not invent a parallel framework to enforce folder names or report prose.

Next: comparison/splits → `held-out-and-guards.md`; grader calibration → `llm-judge.md`; failure repair → `failure-repair.md`.
