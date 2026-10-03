# RFC.md template — decision body

Load when writing `RFC.md`. Freeze it when accepted. Implementation goes in `IMPLEMENTATION.md`, metrics in `KPI.md`, and a live-code audit directly after the header (`references/workflow.md` § Audit).

```markdown
# RFC: {Title}

Status: Draft | In Review | Accepted | Rejected | Superseded
Recommendation: none | final
Decision type: Reversible | Irreversible
Author(s): {names}
Created / Updated: {dates}

## Summary
For a Draft, state the decision being investigated, open blockers and no final recommendation. After blockers close, state the recommended decision and why it matters.

## Goals and Non-Goals
- Goal: {checkable outcome}
- Non-goal: {explicit boundary}

## Motivation and Current State
Problem, affected users/workflows, concrete use cases, current code/process with exact evidence, and cost of doing nothing. Show measured state as a diagram (`xychart-beta` before/after, `pie`/`sankey-beta` for where cost goes).

## Guide-Level Explanation
Teach the proposal through concepts, examples, errors, migration guidance, and documentation impact.

## Reference-Level Explanation
Define architecture, APIs/contracts, interactions, edge cases, compatibility, and reversibility. Link each choice to rationale, alternatives, and risks. Draw flows, protocols and lifecycles (`flowchart`, `sequenceDiagram`, `stateDiagram-v2`, `classDiagram`).

## Drawbacks and Pre-mortem
List cost, complexity, operations, performance, learning, migration, blast radius, failure trigger, and mitigation.

## Rationale and Alternatives
Comparison outcome: unresolved | final
Render the comparison as a table plus a `quadrantChart`/`radar-beta` or a decision `flowchart` when trade-offs span two or more criteria. After blockers close, explain why the recommended design wins on the owner criteria.

## Prior Art
State decision-relevant lessons from local systems, ecosystem implementations, standards, or research.

## Unresolved Questions
Decision blockers: open | none | resolved. Select the truthful value. While any remain, keep `Status: Draft` and `Recommendation: none`, and list each blocker with owner, evidence gap and next check.
Q1: {open decision blocker, if any} — owner / evidence gap / next check
- [ ] {non-blocking execution question} — impact / owner / next proof or deferral trigger
Carry execution questions into `IMPLEMENTATION.md`.

## Future Possibilities
Optional extensions that remain outside this decision.
```

Next: existing code → `references/rfc-prerequisites.md`; settled decision → acceptance via `references/rfc-kpi.md` when warranted, then `references/rfc-implementation.md`.
