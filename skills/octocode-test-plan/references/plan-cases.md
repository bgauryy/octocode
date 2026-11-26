Load when writing the plan's Cases section: the heading, fields and rules of each case.

# Case format

## Cases

Give each case a unique ID, title and fields below. Head each case `<ID> — <title>` one heading level below Cases; the ID is one token with a digit, such as `T1`, and other headings there are notes. Start each field on its own line as `Given: …`, `**Given:** …` or a `| Given | … |` row. A field written once in the Cases intro, before the first case, applies to every case.

| Case field | Required content |
|---|---|
| Covers | Feature/claim and deciding source anchor |
| Priority/level | P0/P1/P2 and unit/integration/end-to-end/manual |
| Given | Setup state, fixtures and dependencies; pin time, randomness, locale, ordering and network, or state the repeat count that decides a flaky case |
| When | Action or ordered scenario; exact command/request/input where syntax or ordering matters. Otherwise state the target state and how to check it |
| Then | Observable comparison: value, property, pattern, status, file, count, or timing bound with unit/baseline/runs |
| Pass if / Fail if | Unambiguous oracle, chosen by the [oracle order](case-oracles.md#write-the-oracle) |
| Discriminates | For P0: the base revision, revert or seeded fault under which the case fails; the plan names it and the executor applies it, the plan edits nothing |
| Evidence to capture | Output, exit code, file or log needed to verify the result |
| Cleanup | Remove case-owned state and verify restoration, or say none |
| Skip if | Optional: the condition under which the executor may report `skipped`; without it, `skipped` is not allowed |

Mark a reused test `existing`: name its `path:line` and the command that selects it; it is coverage, and still needs its own result. Cases run independently unless Given names a dependency. Link repository setup rather than copying it.
