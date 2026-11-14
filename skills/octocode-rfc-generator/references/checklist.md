Load when planning features and tasks, checking that they trace, or finding and closing questions that could block the decision.

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
| Schedule | Critical path | The longest `Depends on` chain is marked; the rest can run in parallel |
| Change a contract | Expand, migrate, contract | New beside old, consumers moved, old removed as its own task; each step ships and reverts alone |
| Replace a component | Strangler fig | One slice at a time moves to the new part; the old serves the rest until its removal task |
| Roll out | Staged rollout | off → limited → default → switch removed, each move gated by a signal and a rollback threshold; skipped when a plain revert contains the risk |
| Hand off a task | [Task contract](protocol-catalog.md#delegate-and-coordinate); SBAR when blocked | The row gives objective, inputs, scope and Definition of Done |
| Report progress (revision) | Task state | `done` cites evidence, `blocked` names its `Q`, the [task graph](diagrams.md) shows both |

## Trace

- One name per thing: each glossary term and entity defined once under its code name, never two words for it.
- Each feature has a task and a `V` check; without a check it is unaccepted.
- Each task serves a feature or prerequisite; otherwise it is scope creep or a missing feature.
- No task depends on a later one; the first is startable now, or its blocker is a `Blocking:` question.
- Each deciding claim cites a source listed in Sources, and each link resolves.

A gap is a finding: add the row or state why it stays out of scope.

## Track and close questions

- Each open question sits beside its decision or in Open questions: what is unresolved, why it matters, supporting and conflicting evidence, owner, next check; `Q1` IDs when cross-referenced.
- A proposed answer (reviewer, provider or judge) is not a resolution; only deciding evidence closes a blocker.
- State: open, contested, resolved, deferred (or the project's terms). Keep the current rationale and material dissent; revisit when evidence or criteria change.

| When a question becomes | Required basis |
|---|---|
| Resolved (fact or causal claim) | Cited, host-inspected current source or executed check establishing the scoped answer and explaining material counterevidence |
| Resolved (design tradeoff) | Verified prerequisites, explicit owner criteria, competing arguments, rationale, remaining risks |
| Resolved (owner preference) | Recorded owner decision or prior instruction; never guessed |
| Deferred | Execution detail only: impact, owner, concrete proof or deadline trigger, why waiting cannot reverse the decision |
| Blocked or contested | Missing evidence, unreconciled contradiction, unavailable owner, failed check or exhausted budget; record the smallest next action |

- Never relabel a decision blocker as an execution detail to pass readiness.
- Scope change: give the rationale; recheck dependent goals.
- A consequential question that survives source review: consider the optional [clasify review](workflow.md#optional-clasify-review).

## Before declaring ready

Every decision blocker resolved, every execution question resolved or validly deferred, the trace without gaps. Otherwise, when authority permits, deliver a draft naming each blocker's owner, evidence gap and next check. Keep material dissent either way.
