# KPI contract
Load before a comparison. Freeze the decision plan so the target cannot move after results. Link the goal a product user sees to observable success, one primary measure, and guardrails; add leading measures only when they help diagnose or speed development.

| Field | Record before baseline |
|---|---|
| Goal | One user-visible outcome |
| Subject | Exact mutable surface; baseline identity and candidate version |
| Primary | Metric, direction, runnable sensor, aggregation, meaningful effect threshold |
| Guardrails | Critical correctness and safety floors; cost and latency bounds; slices that cannot regress |
| Data | Development, validation, sealed-test, regression IDs and provenance; split unit; exposure owner |
| Access | Solver input allowlist, evaluator-only material, workspace and service reset, access checks |
| Harness | Version or hashes of cases, references, graders, schemas, actual prompts, runtime, tools |
| Judge | Calibration evidence, model and prompt version, error tolerances, abstention and adjudication policy |
| Budget | Independent tasks, repeats, attempts and repairs, candidate-selection limit, time/token/cost ceiling |
| Comparison | Pairing and order, uncertainty method, exclusions, retries, missing-data policy, stop rule |
| Decision | KEEP rule for development; ACCEPT/REVERT/INCONCLUSIVE/INVALID for final evidence |

- Record only applicable fields; state a consequential omission (for example, no calibrated judge).
- Keep the plan in the run record, not a separate contract file. A deterministic optimization (exact byte counts) needs only a short run record, not statistics.
- Never fill baseline or target with a plausible number; mark unmeasured values.
- A public self-test score or zero lint errors can be a maintenance guardrail; neither measures unseen task performance.

Next: execute → `references/agent-loop.md`; role access → `references/clean-lab.md`; final decisions → `references/held-out-and-guards.md`.
