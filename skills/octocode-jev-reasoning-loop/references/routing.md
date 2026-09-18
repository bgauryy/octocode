# Research routing

Load when naming the semantic signal and schema after deterministic routing. Why: hypothesis selection, belief updating, claim proof, and assertion grounding need different state.

| Route | Use when | Schema | Result |
|---|---|---|---|
| `hunch_check` | A weak signal exists but no competing deck is ready. | `assets/hunch.schema.json` | Promote or drop the hunch. |
| `hypothesis_triage` | Two to five testable explanations and precommitted branchable checks exist. | `assets/hypothesis-triage.schema.json` | Provisional lead and discriminating check. |
| `decision_review` | A planned action is high-cost, close, or difficult to reverse. | `assets/decision-review.schema.json` | Viability, primary supplied risk, and evidence need. |
| `reflection_delta` | One material observation arrived after a hypothesis check. | `assets/reflection-delta.schema.json` | Effect on prior lead, updated lead, and reframe signal. |
| `disputed_inference` | Evidence is collected for one bounded claim. | `assets/claim-check.schema.json` | Supported, contradicted, insufficient, or conflicting plus basis. |
| `hallucination_gate` | One evidence-backed claim is about to be asserted. | `assets/hallucination-gate.schema.json` | Proceed, qualify, or block. |
| `apply` | Any Jev response succeeded. | `assets/apply-output.schema.json` | Provisional caller-owned action record. |

Do not route exact facts, arithmetic, permissions, dates, versions, empty-evidence assertions, stale evidence, or already-obvious checks to Jev. Do not add a Jev question merely because the primitive can express it: if the answer cannot change the next action, skip it. Run `scripts/route-decision.mjs` first when the boundary is uncertain.

A route can advance only on state change: hunch → named alternatives; triage → frozen prediction then executed observation; reflection → updated, abandoned, or reframed state; claim-check → independently reopened basis; gate → scoped output. One call per crossroad is normal; one follow-up is allowed only after material new evidence. `assets/default-policy.json` sets the hard maximum.

Next: use `references/deliberation.md` before packet construction and `references/research.md` for route-specific execution.
