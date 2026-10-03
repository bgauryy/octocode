# Eval harness
Load when you implement or extend a benchmark runner. `benchmarks/README.md` owns the definition and results layout; `benchmarks/document-answering/README.md` is the public task and judge starter; `benchmarks/skill-smoke/README.md` covers maintenance checks.

## Cases
- Resolve material expert disagreement or mark ambiguity; no ritual reviewer count.

## Runner
- Record inclusion and exclusion decisions. Create no empty logs for unavailable evidence.
- Repairs get real operational errors, not answer hints, and count as extra attempts under the frozen policy.
- Confirm the rendered input fits the host context. Disable silent truncation where possible; else keep evidence of what reached the model. Requested options or token estimates do not prove prompt fidelity.
- Store grader evidence and source and version identities to reproduce.
- Add scripts only for recurring deterministic work; no parallel framework for folder names or report prose.
