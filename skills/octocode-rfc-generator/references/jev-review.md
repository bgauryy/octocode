# Optional `semanticAssess` review inside the RFC workflow

Load at blocker closure or validation when a consequential semantic question remains after cheap checks and the judgment can change the next action. This is a review step in the ordinary RFC workflow, not a separate artifact or skill.

## Availability and limits

Inspect the Octocode catalog once and check host subagent support. Read `references/jev-api.md` for native `semanticAssess` transport, schema discovery, credentials, and result handling. MCP exposes the tool only when `OCTOCODE_JEV_KEY` is configured; CLI invocation without it must report the missing key. A catalog entry is not a successful provider call. With both capabilities available, use exactly two read-only workers; the host submits their arguments through `semanticAssess` to the Jev API. If a capability is missing or fails, continue evidence collection and the host completeness audit, label attempted coverage `unjudged` or `partial`, and retain blockers. Do not impersonate agents or claim a judgment that did not run.

Default budget: two workers, one independent opening and one rebuttal each, at most three consequential question groups and three judge calls, and ten minutes of review. Record a different budget before starting when scope warrants it. Allow one judgment per unchanged crossroad; additional calls require material new evidence or a changed proposal and an explanation of the action they can change. Stop on exhausted budget, unchanged evidence or a missing owner decision. Count failed attempts and retries.

## Review sequence

1. **Frame and question.** Use the existing RFC and ledger from `references/rfc-completeness.md`. Record the revision, scope, owner criteria, proposed answer and deciding evidence. Classify questions as factual, causal, design tradeoff, owner preference or execution detail. Missing evidence needs collection; owner preferences need actual owner criteria or a decision.
2. **Debate.** Read `references/jev-debate.md` before dispatch. Give both workers the same frozen review contract and raw evidence, with advocate and adversarial roles. Collect independent openings before exchanging them, then one rebuttal each. Preserve concessions and disagreement. If inspected evidence settles the question or the workers converge after rebuttal, stop without a judge call.
3. **Judge.** Save the host's intended next action. Freeze an admission gate containing the remaining disagreement, distinct A/B positions, why evidence and direct checks cannot settle it, and distinct actions for support versus rejection. Preserve both agents' arguments, exact question/proposal identity and evidence IDs; run `node scripts/validate-debate.mjs request.json worker-packet.json` from this skill folder before submission. Put the proposal or claim, admission record, both argument rounds, and evidence in one `resources[].context.value`; copy the frozen typed `review.questions` array to `questions`. The host enforces admission and maps the judgment to its next action. Jev assesses the supplied claim or plan; it does not improve accuracy merely by being called.
4. **Verify and update.** Inspect the original deciding evidence or run the selected check. Apply the closure rules in `references/rfc-completeness.md`, record the actual API result separately from the host disposition, and explain disagreement. Changed sources or criteria reopen affected questions. Votes, confidence and agent agreement never establish facts or owner acceptance.
5. **Return to the RFC gates.** Keep Draft while any decision blocker remains. Continue decision, acceptance, planning, artifact validation and delivery through `references/workflow.md`. Preserve the accepted-document audit rules and existing task authority; review does not itself authorize implementation.

## Receipts and contribution

Store raw dispatches, both worker rounds, exact requests/results, revision/hash, requested model, provider-resolved model(s), page-local coverage, usage, elapsed time, and host checks under `<output>/octocode-rfc-generator/{name}/review/`; use the skill's existing scratch directory for temporary packets. Create the cost receipt described in `references/review-cost.md` and validate it with `node scripts/validate-review-cost.mjs receipt.json`. Keep transcripts outside the RFC and credentials out of all receipts. Chat-only work stays in chat.

Deliver changed answers, decisive evidence, remaining questions with owners/next checks, dissent, actual page-local judge coverage, and measured cost. State what the Jev-backed assessment changed relative to the saved pre-call action, distinguishing discovery, prioritization and confirmation; “no demonstrated help” is valid. Provider availability, successful preflight, and host verification are separate claims.

When changing or comparing this step, use `references/jev-evaluation.md`, run `node scripts/validate-debate.mjs --self-test`, and review the skill with `octocode-skills`. For provenance and API ownership use `references/jev-sources.md`.

Next: return to `references/workflow.md` for the existing RFC gates and delivery.
