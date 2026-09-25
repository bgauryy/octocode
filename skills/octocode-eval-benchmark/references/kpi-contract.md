# KPI contract
Load before a comparison. Why: a frozen decision plan prevents moving the target after seeing results. Link the user-visible goal to observable success, a primary outcome measure and relevant guardrails; add leading measures only when they help diagnose or speed development.

| Field | Record before baseline |
|---|---|
| Goal | One user-visible outcome |
| Subject | Exact mutable surface; baseline identity and candidate version |
| Primary | Metric, direction, runnable sensor, aggregation, meaningful effect threshold |
| Guardrails | Critical correctness/safety floors and cost/latency bounds; slices that cannot regress |
| Data | Development, validation, sealed-test, regression IDs/provenance; split unit and exposure owner |
| Access | Solver input allowlist, evaluator-only material, workspace/service reset and access checks |
| Harness | Version/hashes of cases, references, graders, schemas, actual prompts, runtime and tools |
| Judge | Calibration evidence, model/prompt version, error tolerances, abstention/adjudication policy |
| Budget | Independent tasks, repeats, attempts/repairs, candidate-selection limit, time/token/cost ceiling |
| Comparison | Pairing/order, uncertainty method, exclusions, retries, missing-data policy, stop rule |
| Decision | KEEP rule for development; ACCEPT/REVERT/INCONCLUSIVE/INVALID for final evidence |

Record only applicable fields; state a consequential omission (e.g. no calibrated judge). Keep the plan in the run record rather than duplicating it in a separate contract file. For a deterministic optimization, this can be a short run record; do not manufacture statistical machinery for exact byte counts.

Do not fill baseline or target with a plausible number. Mark unmeasured values explicitly. A public self-test score or zero lint errors can be a maintenance guardrail; neither measures an agent's unseen task performance.

Next: execute → `agent-loop.md`; role access → `clean-lab.md`; final decisions → `held-out-and-guards.md`.
