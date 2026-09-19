# Context selection

Load when a question needs several sources or a packet needs trimming. Why: probabilities depend on the supplied evidence; missing or irrelevant material can mislead the judgment.

## Supply the deciding evidence

Include the decision and scope, decisive excerpts, counterevidence, definitions, and material unknowns. Preserve source anchors, negations, guard branches, units, and timestamps. Summaries can explain background but must not replace observations that could refute the host's belief.

Distinguish what an excerpt shows from what the entire file or repository does. `contentRef` resolves a complete selected span or fails before calling Jev: default 1200 characters, maximum 4000. Select a complete smaller span, split independent evidence items, or raise the limit; preserve the deciding branches. Scout excerpts can still be incomplete: inspect coverage and widen retrieval before treating a negative judgment as absence.

Ground multi-part claims separately when their evidence differs. If a conclusion depends on a relationship between sources, include the relevant parts together. Independent per-file scores do not establish a cross-file relationship.

## Bound the request

Use current model limits in `references/protocol.md`. Budget state, instructions, and criteria together, with headroom for estimation error. Bytes and characters are not token counts; actual `usage` is telemetry after a request. Keep decisions small rather than filling the context window.

Deduplicate extracts and shorten background before cutting evidence. If material still does not fit, split by independent propositions or evidence dependencies. Preserve all deciding anchors for a final comparison; otherwise leave the cross-source conclusion unresolved.

## Compose without inventing evidence

Questions in a request are independent. Batch them when they share context. For a dependent question, construct the required state first; alternatively ask a branch-specific question with an explicit premise and consume it only when that branch applies. A prior model answer remains a judgment with uncertainty, not a new fact.

For reasoning routes, a short observations/uncertainty summary is enough unless assumptions or predictions matter. Missing evidence calls for retrieval, not repeated votes or a more flattering packet. Continue with `references/research.md` for runner diagnostics.
