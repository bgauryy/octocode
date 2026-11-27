Load when removing test debt (iteration files, skipped tests, mocks, unused setup) or writing the test that replaces a weak or removed one.

# Test debt and replacement tests

## Test debt

Prove by behavioral overlap and coverage after reading both tests; "the suite still passes" proves nothing. Signal → evidence before delete:

- Iteration file: name ends `-N`/`-N-N`, dated, or version-slugged → every describe block is a strict behavioral subset of an existing base.
- Date-pinned bug test: one dated incident → a domain test covers it.
- Skipped test (`skipped-test` rows) → still required? Restore, replace, or document regression intent (a missing ticket proves nothing).
- Rigid mock: rebuilds a large internal object to check one field, counts internal-helper calls, spies on an unexported symbol → outcome assertable via public API; repair if the behavior matters.
- Mirror mock: `__mocks__` duplicating a production module's shape → real implementation runs in a temp directory.
- Redundant stub: `mockReturnValue` matching the real return, no `expect` on the mock → decorative.
- Unused setup: `beforeEach`/`afterEach` state no `it` in the block touches.
- Environment coupling: `spawnSync` without explicit `env:`, unreset `process.env` reads, cwd dependence → passes with the variable set and unset.

Excision: compare candidates with their retained coverage; keep blocks exercising distinct paths. Delete in reviewable batches and run affected tests. Replace lost coverage (coverage-diff `lost`, below) or revise the deletion; never relax acceptance or widen a mock to pass.

## Replacement tests

- Assert the outcome, not the call: a `db.exec` spy count passes with a wrong schema → query `sqlite_master` on `new DatabaseSync(':memory:')` from `node:sqlite` (Node.js 26.11.0 renamed the class `Database` and keeps `DatabaseSync` as a deprecated alias; earlier versions have only `DatabaseSync`).
- Prompt or doc text: check real parser fields, links and declared limits; evaluate behavior with representative requests when meaning changes. A full-sentence or keyword regex assertion does not prove instruction compliance.
- Isolate and control time, env, cwd, fixtures: explicit `env`/`cwd` for `spawnSync`; `mkdtempSync` dirs removed in `afterEach`; capture each `process.env` value before overwriting, restore or delete in teardown; no shared state; declare platform or service dependencies.
- Name by outcome: `it('test case 3')` → `it('returns 404 when the key is missing')`. Follow the project's domain naming instead of temporary iteration names.
- Minimal fixtures; failures name expected and actual.
- Compare coverage before and after with [coverage-diff](../scripts/coverage-diff.mjs): `node <skill-folder>/scripts/coverage-diff.mjs <before> <after>` (lcov, istanbul or coverage.py JSON; add `--before-root <checkout>` when the baseline ran in another checkout). Exit 1 lists `lost` files with more uncovered lines or branches; a moved file shows as `removed` plus `lost`, so compare those two entries. Compare behavioral cases by reading both tests; restore losses via a public contract.
