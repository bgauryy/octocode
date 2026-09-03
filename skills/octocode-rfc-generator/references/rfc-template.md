# RFC.md template — decision body

Load when writing `RFC.md`. Why: this reviewer-facing document is the single source of truth for goals, scope, and decision; freeze it when accepted.
Implementation detail belongs in `rfc-implementation.md`, metrics in `rfc-kpi.md`, and source inventories in `rfc-resources.md`. On (re)read against live code, insert the audit block right after the header fields: `references/rfc-audit.md`.

```markdown
# RFC: {Title}

Status: Draft | In Review | Accepted | Rejected | Superseded
Decision type: Reversible | Irreversible
Author(s): {names}
Created / Updated: {dates}

## Summary
One paragraph that states the decision and why it matters.

## Goals and Non-Goals
- Goal: {checkable outcome}
- Non-goal: {explicit boundary}

## Motivation and Current State
Problem, affected users/workflows, concrete use cases, current code/process with exact evidence, and cost of doing nothing.

## Guide-Level Explanation
Teach the proposal through concepts, examples, errors, migration guidance, and documentation impact.

## Reference-Level Explanation
Define architecture, APIs/contracts, interactions, edge cases, compatibility, and reversibility. Link each choice to rationale, alternatives, and risks.

## Drawbacks and Pre-mortem
List cost, complexity, operations, performance, learning, migration, blast radius, failure trigger, and mitigation.

## Rationale and Alternatives
Explain why this design wins. Compare viable options, including do-nothing when relevant, on the criteria that can change the decision.

## Prior Art
State decision-relevant lessons from local systems, ecosystem implementations, standards, or research. Put the inventory in `RESOURCES.md`.

## Unresolved Questions
- [ ] {question} — owner / next proof
Each question must be closed with evidence in `IMPLEMENTATION.md` or explicitly deferred with a trigger.

## Future Possibilities
Optional extensions that remain outside this decision.
```

Quality gate: exact citations support non-obvious claims; decision-blocking uncertainty is resolved and remaining uncertainty is explicit; goals and scope appear only here. <!-- style-lint: ignore-line passive-voice -->
Every citation states why it matters; option comparisons render as a markdown table; no filler or duplicate phrasing.

Next: when the change touches existing code load `references/rfc-prerequisites.md`; otherwise close the open questions and build with `references/rfc-implementation.md`.
