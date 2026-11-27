Load when recording, resolving or deferring an open question, or deciding whether the RFC is ready.

# RFC questions and readiness

## Track and close questions

- Each open question sits beside its decision or in Open questions, with the fields of the [Open questions row](../output.md#structure) plus supporting and conflicting evidence; `Q1` IDs when cross-referenced.
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
- A consequential question that survives source review: run the optional [clasify review](workflow.md#optional-clasify-review) when its entry conditions hold.

## Before declaring ready

Every decision blocker resolved, every execution question resolved or validly deferred, the trace without gaps. Otherwise, when authority permits, deliver a draft naming each blocker's owner, evidence gap and next check; the material dissent kept above ships in both.
