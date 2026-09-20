# RFC.md template — decision body

Load when writing `RFC.md`. Why: this reviewer-facing document owns goals, scope, and decision; freeze it when accepted. Implementation belongs in `rfc-implementation.md`, metrics in `rfc-kpi.md`, sources in `rfc-resources.md`, and live-code audit directly after the header via `references/rfc-audit.md`.

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
Problem, affected users/workflows, concrete use cases, current code/process with exact evidence, and cost of doing nothing.

## Guide-Level Explanation
Teach the proposal through concepts, examples, errors, migration guidance, and documentation impact.

## Reference-Level Explanation
Define architecture, APIs/contracts, interactions, edge cases, compatibility, and reversibility. Link each choice to rationale, alternatives, and risks.

## Drawbacks and Pre-mortem
List cost, complexity, operations, performance, learning, migration, blast radius, failure trigger, and mitigation.

## Rationale and Alternatives
Comparison outcome: unresolved | final
During investigation compare conditional tradeoffs, including do-nothing when viable, and name reversal conditions and deciding checks. Do not select an overall winner while blockers remain. After blockers close, explain why the recommended design wins on the owner criteria.

## Prior Art
State decision-relevant lessons from local systems, ecosystem implementations, standards, or research. Put the inventory in `RESOURCES.md`.

## Unresolved Questions
Decision blockers: open | none | resolved. Select the truthful value. While any remain, keep `Status: Draft` and `Recommendation: none`; compare options provisionally and list each blocker with owner, evidence gap and next check. Close every blocker before a final recommendation or readiness claim.
Q1: {open decision blocker, if any} — owner / evidence gap / next check
- [ ] {non-blocking execution question} — impact / owner / next proof or deferral trigger
Carry execution questions into `IMPLEMENTATION.md`; resolve them with evidence or defer them explicitly before Ready for Review.

## Future Possibilities
Optional extensions that remain outside this decision.
```

Quality gate: exact citations support non-obvious claims; Drafts expose blockers, final recommendations resolve them; goals and scope appear only here. Explain why citations matter, render comparisons as a table, and remove filler. Next: existing code → `references/rfc-prerequisites.md`; settled decision → acceptance via `references/rfc-kpi.md` when warranted, then `references/rfc-implementation.md`.
