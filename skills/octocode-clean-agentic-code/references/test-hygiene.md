# Test Hygiene

Load to remove legacy test iterations, skipped tests, rigid mocks, redundant stubs, unused setup, or environment-coupled tests, or to write a replacement test. Prove test debt by behavioral overlap and coverage, not by caller proof. A delete without a replacement is a silent coverage gap. "The suite still passes" satisfies no row.

| Class | Signal | Evidence before delete |
|---|---|---|
| Iteration file | name ends `-N` or `-N-N`, carries a date, or slugs a version | a base file exists, and every describe block in the suffixed file is a strict behavioral subset of it |
| Date-pinned bug test | tests one dated incident | a domain test covers the behavior |
| Skipped test | `it.skip`, `describe.skip`, `xit`, `xdescribe`, `test.skip`, `it.todo` | you decided whether the behavior is still required and restored, replaced, or documented its regression intent; a missing ticket proves nothing |
| Rigid mock | rebuilds a large internal object to check one field, counts calls on an internal helper, or spies on an unexported symbol | the outcome is assertable through the public API; repair it if the behavior matters |
| Mirror mock | `__mocks__` file duplicating a production module's shape | the real implementation runs in a temp directory |
| Redundant stub | `mockReturnValue` matching the real return, no `expect` on the mock | the stub is decorative |
| Unused setup | `beforeEach`/`afterEach` state no `it` in the block touches | nothing consumes it |
| Environment coupling | `spawnSync` without explicit `env:`, unreset `process.env` reads, working-directory dependence | passes with the ambient variable set and unset |

Read the candidate and replacement tests before you judge overlap.

## Excision

1. List candidates; diff each suffixed file against its base.
2. Keep any block that exercises a path the base misses.
3. Delete in one batch; run the package's tests.
4. Replace lost coverage or revise the delete.
5. Never relax acceptance or widen a mock to make a delete look safe.

## Replacement tests

| Property | Rule |
|---|---|
| Behavioral | asserts the contract: output, state transition, side effect, or a required interaction |
| Isolated | own temp directory; restores every env var it sets; no state shared between blocks |
| Deterministic | controls time, env, working directory, fixtures; declares platform or service dependencies |
| Named by outcome | `it('returns 404 when the key is missing')`, not `it('test case 3')` |
| Minimal fixture | only what this assertion needs |
| Readable failure | names expected and actual |

- Assert the outcome, not the call: a spy count on `db.exec` passes with a wrong schema; query `sqlite_master` on a `new DatabaseSync(':memory:')`.
- Match the contract, not the prose: `expect(prompt).toMatch(/agent\s+register/i)` plus a byte budget, not `toContain` on a full sentence.
- Never inherit ambient state: pass explicit `env` and `cwd` to `spawnSync`; create work directories with `mkdtempSync` and remove them in `afterEach`; capture each `process.env` value before you overwrite it and restore or delete it in teardown.
- Name files `<domain>-<what-is-under-test>.test.ts`, never with an iteration number or date.
- Compare coverage and behavioral cases before and after; restore lost coverage through a public contract.

Next: run the batch with [lobby workflow](../SKILL.md#workflow).
