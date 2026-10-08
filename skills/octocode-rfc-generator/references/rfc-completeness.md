# RFC completeness and closure

Load when discovering questions or closing them. Why: one unanswered safety or compatibility question can invalidate a polished RFC, and the closure rules live here.

## Ask from the decision outward

Use the areas that could change the decision or plan. Explain an omitted area only when readers could reasonably expect it to matter.

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

## Track unresolved questions

Keep questions beside the affected decision or in the RFC's open questions section. Capture what remains unresolved, why it matters, supporting and conflicting evidence, an owner, and the next check. Use IDs when cross-references help; no fixed field set is required. A proposed answer is not a resolved answer.

Make open, contested, resolved, and deferred questions distinguishable using the project's labels. Preserve meaningful decision history; revisit answers when their evidence or criteria change.

| Transition | Required basis |
|---|---|
| → resolved (fact/causal claim) | Host-inspected current source or executed check establishes the scoped answer; material counterevidence is explained. State the verified fact and cite its deciding evidence. |
| → resolved (design tradeoff) | Verified prerequisites, explicit owner criteria, competing arguments, rationale and remaining risks. Judge viability can inform the choice but cannot establish those prerequisites. |
| → resolved (owner preference) | Recorded owner decision or prior instruction that actually answers this choice. No agent guesses. |
| → deferred | Execution detail only; record impact, responsible owner, concrete proof/deadline trigger and why waiting cannot reverse the decision. |
| → blocked / contested | Missing deciding evidence, contradictory evidence not reconciled, unavailable owner, failed check or exhausted budget. Record smallest next action. |

Do not convert a decision blocker to an execution detail to pass readiness. A scope change needs a rationale and a check of dependent goals. Agreement between reviewers cannot substitute for deciding evidence.

## Return to the RFC workflow

Before a final recommendation or readiness claim, check that all decision blockers are resolved and execution questions are resolved or validly deferred. A Draft may be saved or delivered with open blockers when task authority permits; name the owner, evidence gap and next check. Preserve material dissent in either state. Distinguish Draft, In Review, and owner-Accepted; accepted status requires actual owner acceptance. Do not create a parallel artifact format; the owners in `references/workflow.md` stay.

Next: consequential unanswered questions may use the [clasify tool](clasify-review.md); completed closure returns to [the workflow](workflow.md).
