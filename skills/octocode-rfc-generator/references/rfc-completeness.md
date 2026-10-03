# RFC completeness and closure

Load when discovering questions or validating the RFC. One unanswered safety or compatibility question can invalidate a polished RFC.

## Ask from the decision outward

For each applicable area, ask a concrete question whose answer could change the decision or plan. Mark genuinely inapplicable areas with a reason rather than manufacturing work.

| Area | Questions to expose |
|---|---|
| Goal and scope | Which user outcome changes? What is excluded? Can the status quo meet the acceptance criteria? |
| Current state | Which current source or test establishes the problem? Are revisions, environment and affected callers known? |
| Contracts | What changes for consumers, stored data, errors, defaults, permissions and compatibility? Who owns each boundary? |
| Failure and recovery | What happens after partial failure, cancellation, retry or duplicate work? What cannot be rolled back? |
| Alternatives | What evidence would make another viable option win? Which assumptions are shared by all options? |
| Delivery | What prerequisites must exist first? Who performs migration, rollout, monitoring and rollback, and on what trigger? |
| Measurement | What baseline, target, guardrail, time window and check decide success? Can the metric improve while the user outcome worsens? |
| Uncertainty | Which unresolved answers could reverse the decision? Which execution details can wait, and until when? |

## One question ledger

Extend the existing claim ledger; assign stable `Q1`, `Q2`, … IDs. Record `question | kind | decision-blocking? + why | proposed answer | evidence + revision | counterevidence | status | owner | next check/trigger | affected RFC section`. Add judge receipt and host verification anchors only when they exist. A proposed answer is not a resolved answer.

Use `open`, `contested`, `blocked`, `resolved` or `deferred`. Preserve history when status changes; source/criteria changes reopen affected resolved entries.

| Transition | Required basis |
|---|---|
| → resolved (fact/causal claim) | Host-inspected current source or executed check establishes the scoped answer; material counterevidence is explained. Record what ran and its result. |
| → resolved (design tradeoff) | Verified prerequisites, explicit owner criteria, competing arguments, rationale and remaining risks. Judge viability can inform the choice but cannot establish those prerequisites. |
| → resolved (owner preference) | Recorded owner decision or prior instruction that actually answers this choice. No agent guesses. |
| → deferred | Execution detail only; record impact, responsible owner, concrete proof/deadline trigger and why waiting cannot reverse the decision. |
| → blocked / contested | Missing deciding evidence, contradictory evidence not reconciled, unavailable owner, failed check or exhausted budget. Record smallest next action. |

Do not convert a decision blocker to an execution detail to pass readiness. A legitimate scope change requires an explicit rationale and rechecking dependent goals. Agreement by both workers cannot substitute for any row above.

## Return to the RFC workflow

Before a final recommendation or readiness claim, check that all decision blockers are resolved and execution questions are resolved or validly deferred. A Draft may be saved or delivered with open blockers when task authority permits; name the owner, evidence gap and next check. Preserve material dissent in either state. Distinguish Draft, In Review, and owner-Accepted; accepted status requires actual owner acceptance. Do not create a parallel artifact format; the owners in `references/workflow.md` stay.

Next: consequential unanswered questions may use `references/jev-review.md`; completed closure returns to `references/workflow.md`.
