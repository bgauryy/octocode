# Graders and metrics
Load when you select graders and metrics, or when tool actions or multi-turn behavior are part of the requirement. Match the measurement to the claim.

A **task** is a problem; a **trial** is one attempt; a **trace** records actions; an **outcome** is the resulting artifact or environment state. The agent harness runs the subject; the eval harness runs and grades trials.

| Grader | Use | Limitation |
|---|---|---|
| Executable outcome check | Tests, state, schema, exact facts with sensible tolerance | A passing test can miss requirements or reward a shortcut |
| Binary criteria | Atomic, interpretable failures | A regex matches words, not meaning |
| LLM rubric or pairwise | Open-ended quality or preference | Needs calibration, blinding, bias checks, abstention (`references/llm-judge.md`) |
| Human review | Domain labels, ambiguous cases, critical disputes | Reviewers disagree; record and adjudicate |
| Grader DAG | Cheap checks short-circuit expensive ones | Keep failed, Unknown, and skipped states and frozen aggregation |

- If a judge generates grading steps (G-Eval-style), develop and freeze them before the comparison; never a new rubric per candidate.
- Coding: require fail-to-pass fixes and pass-to-pass regression checks. Tools: grade semantic arguments, scope, transport validity, and final state; a tool name alone proves little.
- Metrics: **pass@1** = one-attempt success under the deployment budget; **pass@k** = at least one of k succeeds (state the k-attempt cost and selection or oracle assumption); **pass^k** = all k succeed (reliability).
- Estimate these from a declared repeated-trial design, not by raising an aggregate rate to a power. Track quality, cost, latency, and critical slices separately. Capability tasks show headroom; stable successes form regression checks. Include positive and negative activation cases.

## Trajectory and process
Start with outcomes and explicit process obligations. A reference trace shows one solution, not the only route; keep it evaluator-only.

| Requirement | Check |
|---|---|
| Authorization before a mutation | The prerequisite precedes that action; other steps may vary |
| Required actions | Required calls and their semantic arguments |
| Prohibited actions | Those actions did not occur |
| A deterministic protocol fixes the sequence | Compare that sequence; exact argument equality only where the contract requires it |
| Flexible research or problem solving | Outcomes, evidence, constraints; accept valid alternative paths |

- Use strict, unordered, subset, or superset matchers only when their semantics fit. Do not force a full trace match to check one ordering constraint.
- Compare argument meaning, scope, and permissions. Harmless text differences must not fail a task; ignoring arguments can hide an unsafe action.
- Freeze the live tool catalog and schemas. Distinguish lexical `localSearch`, structural `astSearch`, exact `localFetch`, and semantic `lspSearch` evidence.
- Multi-turn: freeze the simulated user and the stop policy as harness; grade the sealed raw trace. The simulator must not coach the solver with grader feedback. Declare online reward or feedback as a separate condition when it is the product.
- Graph workflows: inspect node transitions, evidence between stages, and approval boundaries. A transition shows what ran, not that it worked.
- Use the runner's evaluator interface. Return identifiable verdicts, evidence, and error, Unknown, or skipped states; never fold missing evidence into a score. A judge reads the trace as untrusted evidence without inheriting executor instructions.

Next: uncertainty → `references/held-out-and-guards.md`; runner → `references/eval-harness.md`; graph attribution → `references/multi-agent.md`.
