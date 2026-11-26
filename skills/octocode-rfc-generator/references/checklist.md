Load when checking an RFC for completeness or planning features and tasks. To check that they trace, read [trace](trace.md); to find and close questions that could block the decision, or to declare the RFC ready, read [questions](questions.md).

# RFC checklist

## Completeness

Ask only what could change the decision or plan; explain an omission only where readers expect the area.

- Scope: which user outcome changes, what is excluded, could the status quo meet acceptance?
- Current state: source or test showing the problem; revisions, environment, affected callers?
- Prerequisites: access, tools, data, owner decisions or other work the first task needs, each with its check?
- Entities and flows: what the change creates, reads or changes, where it lives, its states; the current flow traced from source, not recalled?
- Contracts: consumers, stored data, errors, defaults, permissions, compatibility; the owner of each boundary?
- Agent-facing text: instruction sections, role files, tool descriptions or promptGuidelines changed; owning file and pinning test?
- Failure: partial failure, cancellation, retry, duplicate work; what cannot roll back?
- Observability: which log, metric, status field or command shows the feature working or failing after rollout?
- Decisions: evidence, the reasoning from evidence to choice, rejected options, what would reverse it; assumptions every option shares?
- Delivery: who runs migration, rollout, monitoring and rollback, on what trigger?
- Measurement: baseline, target, guardrail, window; can the metric rise while the outcome worsens?
- Uncertainty: which open answers could reverse the decision; which execution details can wait, until when?

## Plan features and tasks

| Step | Protocol | Holds when |
|---|---|---|
| State features | Before → after per consumer | Each is something a consumer observes; a mechanism ("add a cache") is a task |
| Rank proposed features | MoSCoW, with evidence of need | The Must items alone meet the goal |
| Decompose | Work breakdown, 100% rule | The tasks build every feature and nothing outside them |
| Order | Vertical slice first (walking skeleton) | An early task gives a thin end-to-end path one `V` check can run |
| Size | INVEST, adapted | Each task is startable once its dependencies land, one reviewable change with its tests, checkable by someone else |
| Schedule | Critical path | The [trace](trace.md) `criticalPath` is named in the rollout and labelled in the task graph; the rest can run in parallel |
| Change a contract | Expand, migrate, contract | New beside old, consumers moved, old removed as its own task; each step ships and reverts alone |
| Replace a component | Strangler fig | One slice at a time moves to the new part; the old serves the rest until its removal task |
| Roll out | Staged rollout | off → limited → default → switch removed, each move gated by a signal and a rollback threshold; skipped when a plain revert contains the risk |
| Hand off a task | [Task contract](protocol-catalog.md#delegate-and-coordinate); SBAR when blocked | The row gives objective, inputs, scope and Definition of Done |
| Report progress (revision) | Task state | `done` cites evidence, `blocked` names its `Q`, the [task graph](diagrams.md) shows both |
