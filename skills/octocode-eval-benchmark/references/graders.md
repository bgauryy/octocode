# Graders and metrics

Load when you select graders and metrics, or when tool actions or multi-turn behavior are part of the requirement. Match the measurement to the claim.

Terms: **task** = a problem; **trial** = one attempt; **trace** = recorded actions; **outcome** = the resulting artifact or state. The agent harness runs the subject; the eval harness runs and grades trials.

| Grader | Use | Limitation |
|---|---|---|
| Executable outcome check | Tests, state, schema, exact facts with sensible tolerance | Can miss requirements or reward a shortcut |
| Binary criteria | Atomic, interpretable failures | A regex matches words, not meaning |
| LLM rubric or pairwise | Open-ended quality or preference | Needs calibration, blinding, bias checks, abstention (`references/llm-judge.md`) |
| Human review | Domain labels, ambiguous cases, critical disputes | Reviewers disagree; record and adjudicate |
| Grader DAG | Cheap checks short-circuit expensive ones | Keep failed, Unknown, skipped states and frozen aggregation |

- A judge that generates grading steps (G-Eval-style) freezes them before the comparison; never a new rubric per candidate.
- Coding: require fail-to-pass fixes and pass-to-pass regression checks. Tools: grade semantic arguments, scope, transport validity, and final state; a tool name alone proves little.
- **pass@1** = one-attempt success under the deployment budget; **pass@k** = at least one of k succeeds (state the k-attempt cost and selection or oracle assumption); **pass^k** = all k succeed (reliability). Estimate them from a declared repeated-trial design, never by raising an aggregate rate to a power.
- Track quality, cost, latency, and critical slices separately. Capability tasks show headroom; stable successes form regression checks.

## Trajectory and process

Start with outcomes and explicit process obligations. A reference trace shows one solution, not the only route; keep it evaluator-only.

| Requirement | Check |
|---|---|
| Authorization before a mutation | The prerequisite precedes that action; other steps may vary |
| Required actions | Required calls and their semantic arguments |
| Prohibited actions | Those actions did not occur |
| A deterministic protocol fixes the sequence | Compare that sequence; exact argument equality only where the contract requires it |
| Flexible research or problem solving | Outcomes, evidence, constraints; accept valid alternative paths |

- Use strict, unordered, subset, or superset matchers only when their semantics fit; never force a full trace match to check one ordering constraint.
- Compare argument meaning, scope, and permissions. Harmless text differences must not fail a task; ignoring arguments can hide an unsafe action.
- Distinguish lexical `localSearch`, structural `astSearch`, exact `localFetch`, and semantic `lspSearch` evidence.
- Multi-turn: freeze the simulated user and stop policy as harness; grade the sealed raw trace. Declare online reward or feedback as a separate condition when it is the product.
- Graph workflows: inspect node transitions, evidence between stages, and approval boundaries. A transition shows what ran, not that it worked.
- Use the runner's evaluator interface; return identifiable verdicts with evidence and error, Unknown, or skipped states.
