# Test Hygiene

Load when removing legacy test iterations, skipped tests, rigid mocks, redundant stubs, unused setup, or environment-coupled tests, or when writing the replacement test. Prove test debt by behavioral overlap and coverage, not by the caller proof for production dead code. A deletion without a replacement is a silent coverage gap.

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

## Replacement tests

### What a replacement must satisfy

| Property | Rule |
|---|---|
| Behavioral | asserts the relevant contract: output, state transition, side effect, or interaction when that interaction is itself required |
| Isolated | creates its own temp directory, restores any environment variable it sets, shares no mutable state between blocks |
| Deterministic | controls the relevant time, environment, working directory, and fixtures; declares required platform or service dependencies |
| Named by outcome | `it('returns 404 when the key is missing')`, not `it('test case 3')` |
| Minimal fixture | sets up only what this assertion needs |
| Readable failure | the message names what was expected and what arrived |

### Assert the outcome, not the call

The most common rigid-mock repair — the test watched how the work was done instead of what it produced.

```ts
// Wrong — passes even when initDb writes the wrong schema
const spy = vi.spyOn(db, 'exec').mockReturnValue(undefined);
initDb(db);
expect(spy).toHaveBeenCalledTimes(3);
// Right — fails when the schema is wrong
const db = new DatabaseSync(':memory:');
initDb(db);
const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
expect(tables.map(t => t.name)).toContain('work_presence');
```

### Match the contract, not the prose

```ts
// Wrong — breaks on any wording edit, passes when the command is gone
expect(prompt).toContain('Run `agent register --agent-id`');
// Right — holds the concept and the budget
expect(prompt).toMatch(/agent\s+register/i);
expect(Buffer.byteLength(prompt, 'utf8')).toBeLessThanOrEqual(5_000);
```

### Isolate ambient state

One principle covers subprocess, filesystem, and environment tests: never inherit what you did not set. Pass explicit `env` and `cwd` to `spawnSync`, create work directories with `mkdtempSync` and remove them in `afterEach`, and capture any `process.env` value before overwriting it so teardown can restore or delete it. A test that passes only because the host exported a variable is not evidence. Name files `<domain>-<what-is-under-test>.test.ts`, never with an iteration number or a date.

### Coverage replacement rule

Compare coverage and behavioral cases before and after deletion. Restore useful lost coverage through an appropriate public contract.

Next: for the queries that find these, use the symbol-evidence route in `SKILL.md`; to run the batch, load `references/cleanup-playbook.md`.
