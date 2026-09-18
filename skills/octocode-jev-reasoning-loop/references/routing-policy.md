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
| Bounded claim has collected evidence | `disputed_inference` |
| Action is high-cost, difficult to reverse, or judgment is close | `decision_review` |
| Two to five hypotheses have branchable checks | `hypothesis_triage` |
| Weak signal is not yet a hypothesis deck | `hunch_check` |
| Facts or discriminating checks are missing | `missing_fact` |

Defaults live in `assets/default-policy.json`. They are Octocode policy, not Jev semantics: soft tie `< 0.15`, clear lead `> 0.40`, grounded minimum `0.5`, at most two calls per crossroad, and mandatory review for high-cost actions. Calibrate them through `references/benchmark.md`; do not bury replacement numbers in prompts.

The route priority is intentional: deterministic blocks run before semantic judgment, reflection runs only after state change, and claim formation stays separate from assertion grounding.

Next: build a selected Jev route through `references/research.md`; deterministic and missing-fact outcomes return directly to host retrieval or execution.
