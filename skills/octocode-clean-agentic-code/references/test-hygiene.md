# Test Hygiene

Load when removing legacy test iterations, skipped tests, rigid mocks, redundant stubs, unused setup, or environment-coupled tests. Why: test debt is proved by behavioral overlap and coverage, not by the caller proof that governs production dead code.

Deleting a test removes evidence. Every row below states what must be true before the delete, and no row is satisfied by "the suite still passes".

## Smell classes

| Class | Signal | Evidence required before delete |
|---|---|---|
| Iteration file | filename ends `-N` or `-N-N`, carries a date, or slugs a version | a base file exists and every describe block in the suffixed file is a strict behavioral subset of it |
| Date-pinned bug test | file tests one dated incident | the behavior is covered by a domain test |
| Skipped test | `it.skip`, `describe.skip`, `xit`, `xdescribe`, `test.skip`, `it.todo` | determine whether the expected behavior remains required; restore, replace, or document it before removing useful regression intent |
| Rigid mock | rebuilds a large internal object to check one field, asserts call counts on an internal helper, or spies on an unexported symbol | the same outcome is assertable through the public API |
| Mirror mock | `__mocks__` file that duplicates a production module's shape | the real implementation runs in a temp directory instead |
| Redundant stub | `mockReturnValue` matching the real return, with no `expect` referencing the mock | the stub is decorative |
| Unused setup | `beforeEach`/`afterEach` creates or restores something no `it` in the block touches | nothing in the block consumes it |
| Environment coupling | `spawnSync` without explicit `env:`, direct `process.env` reads with no reset, or dependence on the working directory | the test passes with the ambient variable set and unset |

## Confidence

Delete a redundant test when equivalent behavior and regression cases remain covered. A missing ticket or comment is not proof that a skipped test has no value. Repair a rigid test when its behavior still matters. Read candidate and replacement tests before judging their overlap.

## Excision

1. List candidates, then diff each suffixed file against its base before judging it.
2. Keep any block exercising a path the base file misses.
3. Delete files or blocks in one batch, then run the package's tests.
4. Preserve required coverage floors. Replace lost behavior coverage or revise the deletion; a threshold reduction is not a cleanup verification step.

Do not relax acceptance or widen a mock to make a deletion look safe.

Next: for the queries that find these, use the symbol-evidence route in `SKILL.md`; for replacements, load `references/test-quality.md`; to run the batch, load `references/cleanup-playbook.md`.
