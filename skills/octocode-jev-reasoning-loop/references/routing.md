# Research routing

Load when choosing an evidence-based reasoning route and its input state. Native `source_questions` instead takes paths and independent claims before host body reads; use the SKILL.md example without a packet. The table below describes the standalone/evidence routes. Why: hypothesis selection, belief updating, claim assessment, and assertion grounding ask different questions.

| Route | Use when | Schema | Result |
|---|---|---|---|
| `hunch_check` | A weak signal exists but no competing deck is ready. | `assets/hunch.schema.json` | Promote or drop the hunch. |
| `hypothesis_triage` | Two to five testable explanations and precommitted branchable checks exist. | `assets/hypothesis-triage.schema.json` | Provisional lead and discriminating check. |
| `decision_review` | The next action is to execute, revise, or reject a supplied plan that is high-cost, close, or difficult to reverse. | `assets/decision-review.schema.json` | Viability, primary supplied risk, and evidence need. |
| `reflection_delta` | One material observation arrived after a hypothesis check. | `assets/reflection-delta.schema.json` | Effect on prior lead, updated lead, and reframe signal. |
| `disputed_inference` | Evidence is collected for one bounded claim. | `assets/claim-check.schema.json` | Supported, contradicted, insufficient, or conflicting plus basis. |
| `hallucination_gate` | A semantic grounding check could change whether or how one evidence-backed claim is stated. | `assets/hallucination-gate.schema.json` | Proceed, qualify, or block. |

Choose the judgment object before the grammar: a migration, architecture, rollout, or other proposal remains `decision_review` even if restated as the claim “this plan is ready.” Use `disputed_inference` only when the required output is the evidential status of one bounded factual or causal proposition.

Do not route exact facts, arithmetic, permissions, dates, versions, empty-evidence assertions, stale evidence, or already-obvious checks to Jev. Do not add a Jev question merely because the primitive can express it: if the answer cannot change the next action, skip it. Run `scripts/route-decision.mjs` first when the boundary is uncertain.

A route can advance only on state change: hunch → named alternatives; triage → frozen prediction then executed observation; reflection → updated, abandoned, or reframed state; claim-check → verified original basis; gate → scoped output. Reuse an already inspected, complete and current basis; retrieve it when changed, incomplete or unseen. The current policy permits one judgment per crossroad. Materially changed state or a new decision can create a new crossroad only when another judgment would change the next action; do not reset the counter to repeat an unchanged vote. `assets/default-policy.json` sets the hard maximum.

The runner checks direct lookups, missing or stale evidence, incompatible scope, and exhausted call budgets before inference. It builds, validates and applies the response internally; do not add separate host calls for those phases. The action contract is `assets/apply-output.schema.json`. `assets/default-policy.json` owns thresholds and limits, which are local policy rather than Jev guarantees.

Supply `reasoning` when observations, uncertainty, assumptions, or counterevidence clarify the state. The runner derives a minimal summary when omitted. Predictions and branch outcomes belong to testable hypotheses; do not manufacture them for classification. The low-level intermediate contract is `assets/decision-brief.schema.json`.

Next: run compact input through `scripts/run-loop.mjs`; use `references/research.md` for packet debugging and individual commands.
