# Deterministic routing policy

Load when deciding whether a Jev call can change the next action. Why: exact facts, known checks, budgets, and assertion blocks belong in code.

Run `node scripts/route-decision.mjs --input <routing-state.json>`. The output names the route, expected question types, host policy action, and whether assertion is permitted. It never calls Jev.

| Observable condition | Route or action |
|---|---|
| Jev cannot change the next action | `no_jev` → act without Jev |
| Exact lookup or test is available | `deterministic` → run it |
| Assertion has no evidence | `deterministic` → block |
| Claim scope matches no evidence scope | `deterministic` → narrow or block |
| Evidence is stale | `missing_fact` → refresh |
| Material new evidence follows a hypothesis check | `reflection_delta` |
| Assertion has compatible evidence | `hallucination_gate` |
| The next action is to execute, revise, or reject a high-cost, difficult-to-reverse, or close proposal | `decision_review` |
| The output needed is the status of one bounded factual or causal claim with collected evidence | `disputed_inference` |
| Two to five hypotheses have branchable checks | `hypothesis_triage` |
| Weak signal is not yet a hypothesis deck | `hunch_check` |
| Facts or discriminating checks are missing | `missing_fact` |

Defaults live in `assets/default-policy.json`. They are Octocode policy, not Jev semantics: soft tie `< 0.15`, clear lead `> 0.40`, grounded minimum `0.5`, at most two calls per crossroad, and mandatory review for high-cost actions. Calibrate them through `references/benchmark.md`; do not bury replacement numbers in prompts.

Classify the object being judged before filling route fields. Plan viability is a `decision_review` even when someone can phrase it as “the plan is ready”; `disputed_inference` is for a proposition whose supported/contradicted/insufficient/conflicting status is itself the needed output. Deterministic blocks still run before semantic judgment, reflection runs only after state change, and claim formation stays separate from assertion grounding.

Next: build a selected Jev route through `references/research.md`; deterministic and missing-fact outcomes return directly to host retrieval or execution.
