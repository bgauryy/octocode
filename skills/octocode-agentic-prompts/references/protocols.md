Load when an instruction describes a handoff, review, decision, reasoning procedure, stop rule, or output in ad-hoc, rigid, or repeated prose.

## Rules vs presentation

"Keep every rule" protects conditions, thresholds, outcomes, identifiers, and constraints. Presentation is free to change; name each change in your change note:

- **Two scales for one judgment** (urgency and impact, type and importance, priority and severity): make one the source of truth and derive the other wherever the original links them. Never invent a mapping it leaves open: keep that judgment and list the missing pairs in the change note for the owner. Drop the derived field only when you own every reader; otherwise keep it and propose the merge. Scales with separate readers that judge different things stay independent.
- **A thinking order or template** ("first list causes, then evidence for and against", fixed headings, "STEP 1…"): drop it; keep what the answer must contain (the conclusion, its evidence, the check that would refute it). Keep an order only when it guards an action in the world (preflight before deploy).

```text
Before: Set Priority (URGENT, HIGH, NORMAL, LOW) and Severity (S1-S4). S1 = data loss or outage. URGENT for S1. Only S1 or S2 page on-call.
After:  Judge Severity: S1 data loss or outage; retain S2-S4 definitions from their owner. S1 implies URGENT; other priority mappings are unspecified. S1 and S2 page on-call. Change note: derive only the stated mapping; keep the other priorities until the owner resolves them.

Before: Think in this exact structure: list all causes; for each, "Evidence for:" and "Evidence against:"; then pick one.
After:  Answer with the most likely cause, the measurement that supports it, and the result that would show it is not the cause. Change note: thinking template dropped; the answer requirements keep its purpose.
```

## Protocols

- Use a protocol to remove duplication, merge scattered rules, or fill a missing slot (a stop rule, a blocking flag, a rival check); never to relabel a list that is already clear.
- Slots carry the receiver's concrete words ("tests run, exit codes"), not abstract ones ("background"); the protocol name is an optional prefix, never the content.
- Each slot says where it shows up in the output ("prefix the comment `[blocking]`").
- Plain-word slots; define an ambiguous one (MoSCoW "won't": this release or never).
- Project specifics stay verbatim (paths, commands, checks); a protocol never replaces a tool contract, schema, or checked field.
- One owner per protocol line; other files point to it.

## Catalog

Use the shared [protocol catalog](protocol-catalog.md#catalog) for names, slots, and fit. This page owns instruction rewriting; the catalog owns protocol definitions.

## Rewrite rigid text

Rigid: all-caps emphasis; exact fields, orders, or word limits with no consumer; one rule in several places; long lists a protocol covers; ordered steps where order does not matter; and the two presentation cases above.

1. Inventory the passage's rules; mark each line rule or presentation.
2. Apply [rules vs presentation](#rules-vs-presentation); then pick a protocol only where it merges or fills a slot, with the project's specifics in its slots.
3. Definition of Done: every rule survives (in a slot or behind a pointer), checked fields are unchanged, the change note names each presentation change, and a reader new to the protocol could act on it.

```text
Before: Carry: goal, scope, result, evidence, gaps, next action, constraints, next-call identifiers.
After:  Handoff packet: goal and scope as current state; result and evidence; confidence and gaps as blockers; next action, constraints, next-call ids.
```
