# Eval harness
Load when you implement or extend a benchmark runner. Reliable execution and records make scores interpretable.

`benchmarks/README.md` owns the definition and per-run results layout; `benchmarks/document-answering/README.md` is the public task and judge starter; `benchmarks/skill-smoke/README.md` covers maintenance checks.

## Cases
- Keep task IDs, provenance, family and split metadata, and evaluator criteria in the controller; export only the allowlisted solver packet (`references/clean-lab.md`).
- Build cases from representative tasks and observed failures. Check solvability, valid alternatives, and deliberately wrong outputs before you trust the grader. Resolve material expert disagreement or mark ambiguity; no ritual reviewer count.
- Use executable checks for objective properties and calibrated judgment for subjective ones. Version task, fixture, and grader changes; keep original outcomes. Never change the harness mid-comparison to make the subject pass.

## Runner
- Create a unique results directory first. Record effective settings and identities, actual exported inputs, and inclusion or exclusion decisions. Do not create empty logs for unavailable evidence.
- Start independent trials with reset state and production tools. Freeze schemas, permissions, runtime and model versions, and budgets. Keep legitimate within-trial interactions.
- Grade tool scope, semantic arguments, schema and transport validity, and completion separately when relevant.
- Distinguish first attempts, repairs, solver failures, judge errors, infrastructure errors, and Unknowns. Repairs get real operational errors, not answer hints, and count as extra attempts under the frozen policy.
- Confirm the rendered input fits the host context. Disable silent truncation where possible; else keep evidence of what reached the model. Requested options or token estimates do not prove prompt fidelity.
- Store grader evidence and source and version identities to reproduce. Missing records stay visible in coverage and denominators.
- Native runner logs can replace the suggested JSON files if they keep these properties. Add scripts only for recurring deterministic work; no parallel framework for folder names or report prose.

Next: splits → `references/held-out-and-guards.md`; grader calibration → `references/llm-judge.md`; failure repair → `references/failure-repair.md`.
