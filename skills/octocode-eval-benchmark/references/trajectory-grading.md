# Trajectory grading
Load when tool actions or multi-turn behavior are part of the requirement. Why: a correct final answer can still violate permissions or skip a necessary action.

Start with outcomes and explicit process obligations. A reference trace is evidence of one solution, not automatically the only valid route. Keep reference trajectories evaluator-only.

| Requirement | Suitable check |
|---|---|
| Authorization before a mutation | Verify the prerequisite before that action; unrelated steps may vary |
| Particular actions are necessary | Check required calls and their semantic arguments |
| Certain actions are prohibited | Check those actions did not occur |
| A deterministic protocol specifies the whole sequence | Compare the specified sequence; use exact argument equality only where the contract requires it |
| Flexible research or problem solving | Judge outcomes, evidence and relevant constraints; accept valid alternative paths |

Strict/unordered/subset/superset matchers can implement these checks when their semantics fit. Do not force a complete trace match merely to check one ordering constraint. Compare argument meaning, scope and permissions; harmless textual differences should not fail a task, while ignoring arguments can conceal an unsafe action.

For multi-turn simulation, freeze the user simulator and stopping policy as part of the harness. Preserve the raw interaction and grade the sealed trace. The simulator follows its role and scenario; it must not coach the solver with private grader feedback. If online reward or feedback is itself the product under test, declare that separate condition explicitly.

For graph workflows, inspect node transitions, evidence passed between stages, and approval boundaries when relevant. A state transition is evidence of what ran, not proof the action achieved its intended effect.

Use the runner's evaluator interface. Return identifiable verdicts, evidence and error/Unknown/skipped states; do not collapse unavailable evidence into a quality score. A judge may read the recorded trace as untrusted evidence without inheriting executor instructions.

Next: general grader choice → `eval-techniques.md`; model grading → `llm-judge.md`; graph attribution → `graph-of-loops.md`.
