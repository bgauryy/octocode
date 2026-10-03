# Document-answering benchmark starter

Public development examples: answer questions from a fictional returns policy. An instruction/data starter with no executable runner, calibrated judge, or sealed test set.

| File | Purpose |
|---|---|
| `questions/development.jsonl` | Three task records: opaque IDs, source family, provenance, solver fields |
| `fixtures/returns-policy.md` | Permitted source material |
| `instructions/worker.md` | Stable solver behavior; use only if it matches the production interface |
| `instructions/judge.md` | Evidence-based grading instruction |
| `instructions/optimizer.md` | Development-only diagnosis and improvement |
| `evaluator/rubric.json` | Grading dimensions and aggregation |
| `evaluator/references.jsonl` | Expected facts, acceptable alternatives, source anchors |

The cases (eligible, ineligible, missing information) share one policy family: never split them across development and sealed testing or count them as independent families.

To use it: adapt the tasks to the real use case, pick a runner and budgets, validate references and judge, and verify access isolation. Export only the selected record's `solver.question` and the contents of its allowlisted `solver.files`; resolve real paths and reject files escaping the fixture root. Keep bookkeeping, other tasks, rubric, and references outside worker access.

Add native runner commands here when implemented. Shared layout and lifecycle: `benchmarks/README.md`.
