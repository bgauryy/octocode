# Document-answering benchmark starter

Public development examples for answering questions from a fictional returns policy. This is an instruction/data starter; it has no executable runner, calibrated judge, or sealed test set.

| File | Purpose |
|---|---|
| `questions/development.jsonl` | Three task records with opaque IDs, source family, provenance and solver fields |
| `fixtures/returns-policy.md` | Permitted source material |
| `instructions/worker.md` | Stable solver behavior; use only if it matches the production interface |
| `instructions/judge.md` | Evidence-based grading instruction |
| `instructions/optimizer.md` | Development-only diagnosis and improvement |
| `evaluator/rubric.json` | Grading dimensions and aggregation |
| `evaluator/references.jsonl` | Expected facts, acceptable alternatives and source anchors |

The examples cover eligible, ineligible and missing-information cases. All share one policy family: do not split them across development and sealed testing or count them as independent families.

To use the starter, adapt the tasks to the real use case, select a runner and budgets, validate the references/judge, and verify access isolation. Export only the selected record's `solver.question` and the contents of its allowlisted `solver.files`. Resolve real paths and reject files escaping the fixture root. Keep the record's bookkeeping, other tasks, rubric and references outside worker access.

The judge receives the task, source, sealed answer, rubric and matching reference. The optimizer receives development evidence and the allowed subject; no final-test feedback or coaching of scored workers. Grade correct alternatives by meaning, not exact reference wording.

For each real execution, use `.octocode/benchmarks/document-answering/results/<run-id>/` under the chosen workspace/home output root. Keep the effective run configuration, outputs and grades there; add native runner commands to this README when implemented. Follow `benchmarks/README.md` for the shared layout and result lifecycle.
