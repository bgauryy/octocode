# Output

Follow the [presentation rules](references/presentation.md).

BLUF in chat: the verdict, then commands and what they returned, each claim beside its evidence. Name anything unverified as unknown. A plain task writes no file.

Unfinished work: handoff packet (state, evidence, blockers, next action, constraints, next-call ids). A blocker someone else must act on: SBAR.

## Saved report

An audit writes `<repo>/.octocode/octocode-dev/<tool>-<date>.md` from [the template](assets/audit-report.md). Each finding is one 5W2H record, the template's fields being the slots: what (title), where (`file:line`), when (repro call), who (owner, surface), why (cause), how (fix), how much (severity); expected vs observed is a Given-When-Then check. Write `unknown` in an unverified slot.

Protocols: [shared catalog](references/protocol-catalog.md#catalog).
