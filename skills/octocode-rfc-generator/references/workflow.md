Load when planning evidence, auditing an RFC, or a disagreement survives source review. [output](../output.md) owns structure; `octocode-research` owns tools and verification.

# RFC evidence, audit and clasify review

## Gather evidence

- Per [checklist area](checklist.md#completeness) that could change the decision: current source, callers, tests; local constraints, prior art, primary docs for options; real consumers and reversible steps for a migration; a dependency's current release, maintained source, license, integration points, limits; a reproducible baseline, and a benchmark when measurement decides.
- Deciding claim: verify in the original source; search snippets are leads.
- Doubtful or conflicting deciding claim: Hypothesis test: claim, strongest rival, distinguishing check, result (confirmed, dismissed, unresolved); unresolved stays an open choice.
- Keep material counterarguments.

## Audit an existing RFC

- Read the proposal and its support; verify the live code it describes.
- Find implemented work, wanted gaps, contradictions, superseded choices and [trace](trace.md) gaps; recommend keep, update, or retire, with deciding evidence. Feedback: objection (blocks; states the consequence) vs concern (non-blocking). Shipped work that diverged: After Action Review: intended, happened, why different, next.
- Authorized edits: update substance in place, keep accepted decisions, make the current decision clear; no audit headers, transcripts, diary, or review metadata.
- Archive or delete only within the user's scope; update references.

## Optional clasify review

Only when a consequential disagreement survives source review, no direct check settles it, and a typed judgment could change the next action; ordinary evidence review is the default.

- Keep RFC revision, criteria, source evidence, proposed answers, and gaps together; [closure rules](questions.md#track-and-close-questions) still apply.
- Delegated reviewers (when authorized): Delphi: same evidence, independent first positions, then share; keep dissent. Task contract for each: objective a different question, inputs the shared evidence, constraints read-only (you keep write authority and judgment), output a position; keep partial returns.
- Credential: `OCTOCODE_TYPESAFE_KEY` or `classification.api` in `.octocoderc`; use [shared configuration handling](configuration.md).

1. Name the next action this judgment could change; inspect the live query: `octocode schema clasify --view query`. Check credentials without displaying them; missing access leaves review partial, never approved.
2. Submit typed questions over the pinned evidence and competing arguments, including an uncertainty outcome. Inspect every page, error and coverage limit. Retry only for a useful repair or new evidence; otherwise preserve the partial result.
3. Verify the deciding original source before updating the same RFC with conclusion, dissent and uncertainty. Confidence is not correctness; a provider answer grants no implementation authority. Omit requests, receipts, timing and usage.
- Measuring whether assisted review helps: `octocode-agentic`; no evaluation report in the RFC.

Next: find remaining gaps with the [checklist](checklist.md#completeness) and close each one per [questions](questions.md#track-and-close-questions).
