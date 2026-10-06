# RFC.md template — decision body

Load when writing `RFC.md`. Why: these headings are the RFC output. Chat and the file use this list and no other names.

Required, in this order. `scripts/validate-rfc.mjs` rejects a missing heading.
Summary, Goals and Non-Goals, Motivation and Current State, Drawbacks and Pre-mortem, Rationale and Alternatives, Unresolved Questions.

Include an optional heading only when it holds a deciding fact. Omit the heading otherwise.
Guide-Level Explanation (adopters need a teaching path). Reference-Level Explanation (the decision changes a design, API, contract, or data shape). Prior Art (a local system, standard, or implementation changes the choice). Future Possibilities (an extension stays outside this decision).

Header fields use this spelling in chat and in the file: `Status`, `Recommendation` (`none` or `final`), `Decision type` (`Reversible` or `Irreversible`). `Comparison outcome` (`unresolved` or `final`) sits under Rationale and Alternatives. `Decision blockers` (`open`, `none`, or `resolved`) sits under Unresolved Questions.

Lock Goals and Non-Goals before `Recommendation: final`. A blocked Draft keeps `Recommendation: none` and `Comparison outcome: unresolved`.
When the plan stays in this file, append the headings from `references/rfc-implementation.md` after Unresolved Questions.

```markdown
# RFC: {Title}

Status: Draft | In Review | Accepted | Rejected | Superseded
Recommendation: none | final
Decision type: Reversible | Irreversible
Author(s): {names}
Created / Updated: {dates}

## Summary
For a Draft, state the decision being investigated and the open blockers. After blockers close, state the recommended decision and why it matters.

## Goals and Non-Goals
- Goal: {checkable outcome}
- Non-goal: {explicit boundary}

## Motivation and Current State
Problem, affected users or workflows, concrete use cases, current code or process with exact evidence, and the cost of doing nothing. Show measured state as a diagram (`xychart-beta` for before and after, `pie` or `sankey-beta` for where cost goes).

## Drawbacks and Pre-mortem
List cost, complexity, operations, performance, learning, migration, blast radius, the failure trigger, and the mitigation.

## Rationale and Alternatives
Comparison outcome: unresolved | final
Include the status quo when it is a real option. Render the comparison as a table plus a `quadrantChart`, `radar-beta`, or decision `flowchart` when trade-offs span two or more criteria. While blockers are open, keep the outcome `unresolved` and put the reversal condition beside each option. After blockers close, explain why the recommended design wins on the owner criteria.

## Unresolved Questions
Decision blockers: open | none | resolved
While any remain, keep `Status: Draft` and `Recommendation: none`. List each blocker with owner, evidence gap, and next check.
Q1: {open decision blocker, if any} — owner / evidence gap / next check
- [ ] {non-blocking execution question} — impact / owner / next proof or deferral trigger
Carry execution questions into the plan headings in this file. Move them to `IMPLEMENTATION.md` only when the build leaves the RFC.
```

Optional headings. Use the same `##` names. Do not add an empty one.

```markdown
## Guide-Level Explanation
Teach the proposal through concepts, examples, errors, migration guidance, and documentation impact.

## Reference-Level Explanation
Define architecture, APIs and contracts, interactions, edge cases, compatibility, and reversibility. Link each choice to rationale, alternatives, and risks. Draw flows, protocols, and lifecycles (`flowchart`, `sequenceDiagram`, `stateDiagram-v2`, `classDiagram`).

## Prior Art
State decision-relevant lessons from local systems, ecosystem implementations, standards, or research.

## Future Possibilities
Optional extensions that remain outside this decision.
```

Next: existing code → `references/rfc-prerequisites.md`; settled decision → acceptance via `references/rfc-kpi.md` when warranted, then `references/rfc-implementation.md`.
