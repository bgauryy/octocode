# Decision composition

Load when turning typed judgments into application behavior. Why: Jev supplies independent evaluations; code owns dependencies, thresholds and effects.

## Work backward from behavior

Name what the application will show, select, change or hand off. Derive the smallest semantic judgments that behavior needs. Keep rules, exact lookups, calculations, authorization and execution in code; use Jev only where meaning or context determines the answer.

Prefer selection over generation. Supply caller-owned handlers, values, source spans or controls; let Jev choose among them; then copy, normalize or execute through code. Add `other` or `none` when coverage may be incomplete. Never ask Jev to select a candidate the caller omitted, and do not chain choices to synthesize text.

## Choose a composition pattern

- **Route and fill:** use Choice over real handlers, then validate or collect the selected handler's typed arguments.
- **Score reusable dimensions:** ask separate Scores for independently useful factors; retain raw answers so code or user controls can change weights without rerunning unchanged judgments.
- **Retrieve and judge:** retrieve a bounded candidate set first, then evaluate relevance per candidate or select a source-backed value.
- **Verify and escalate:** check a scoped field or claim against supplied evidence; route uncertain, failing or high-stakes cases to a person or reasoning model.
- **Respond to changing state:** keep goals and observations in code, apply one bounded decision, then observe again. Distinguish observed facts from inferred state and refresh stale inputs.

If the next question depends on the winning route, either make a second call with that result in state or precompute independent conditional questions and consume only the applicable answer. State each condition in the question; answers cannot see one another. Track accuracy on representative examples, including ambiguous and hostile inputs, before treating confidence as an automation threshold.

## Apply uncertainty and policy

Choice and Score confidence summarizes probability concentration; it does not certify workflow correctness or authorize an action. Noul near 0.5 means similar probability for yes and no, not medium intensity. Set thresholds from representative user data and the consequence of error. Keep policy explicit: weighted scores fit compensating preferences, while an “any serious violation” rule needs separate conditions. Ignore uncertainty on branches code will not consume.

## Browser pattern from jev-ultrafast

This is an application pattern, not an additional Jev endpoint. The reference implementation supplies visible page text, indexed DOM controls and recent actions. It asks for an operation and independent conditional targets in one request. A separate generative model supplies text when the chosen operation requires it.

1. Observe the current page and retain caller-owned references to actual controls. Exclude sensitive values and unavailable actions. Cap candidates below Choice's 255-option limit; the reference caps actions at 250.
2. Ask a Choice for the operation, plus a target Choice for each supported operation: for example, the best clickable control **if clicking is the next action**, and the best editable field **if typing is next**. Include DONE/BLOCKED in the operation candidates when appropriate.
3. Read the chosen operation, then consume only its corresponding target answer. The CLI structurally validates every returned question; the application's execution branch uses only the selected target. An unused speculative answer never causes an action.
4. Resolve IDs through the original candidate table. Recheck page identity, relevant state and target availability before mutation; a stale observation requires a fresh evaluation. Never convert labels into arbitrary scripts or selectors.
5. Execute one authorized action, record its outcome, and observe again. Set an explicit step/time budget and stop on repeated no-progress states. Retry an evaluation if allowed; do not blindly replay a click or text entry.

The reference uses recent history limited to ten actions, visible text capped at 6,000 characters, and control checks immediately before execution. Those are application tuning choices, not Jev API limits. Screenshots belong to its visualization layer and are not sent to Jev. This skill neither installs a browser driver nor implements the execution loop.

For known model limitations: use literal, aligned instructions and criteria; reduce indirection and irrelevant state. Adversarial state can still influence the answer, so confidence alone cannot enforce a security boundary. Always enforce allowed actions and authorization in caller code.

## Diagnose before retuning

Test representative cases and the resulting application behavior. For a failure, inspect the exact state and candidate coverage, question and criteria, returned probabilities, composition policy, caller code and service outcome. Separate missing evidence, model errors, code errors and transport failures; change the owning layer instead of making the prompt absorb every failure.

When auditing the example's implementation choices, use `references/references.md` for pinned source links.
