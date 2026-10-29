# Output

Follow the [shared presentation and HTML delivery rules](references/presentation.md); retain this skill’s artifact format.

BLUF in chat: the verdict, then the commands and what they returned, each claim beside its evidence (claim–evidence). Name anything not verified as unknown. A plain task writes no file.

- Unfinished work: Handoff packet (current state, evidence, blockers, next action, constraints, next-call ids).
- A blocker someone else must act on: SBAR.

## Saved report

An audit writes one report at `<repo>/.octocode/octocode-dev/<tool>-<date>.md`, using [the audit template](assets/audit-report.md).

Each finding is one 5W2H record, and the template's fields are its slots: what (title), where (`file:line`), when (repro call), who or what it affects (owner, surface), why (cause), how (fix), how much (severity). The expected and observed result is a Given–When–Then check. Write `unknown` in a slot nobody verified.

Protocol definitions: [shared catalog](references/protocol-catalog.md#catalog).
