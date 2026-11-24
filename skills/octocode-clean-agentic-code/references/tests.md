Load when a change edits tests, fixtures, graders, CI, or thresholds; production code reacts to being tested; or test debt is removed or replaced.

# Tests

## Gaming

When checks change with the code they check, compare the diff with the owning contract. Escalate confirmed weakening via the [escalation protocol](agent-defects.md#escalation-protocol).

Literal signals come from [scan-signals](../scripts/scan-signals.mjs): `node <skill-folder>/scripts/scan-signals.mjs --diff <base> --category test-gaming,ci-threshold,weak-oracle`. Judge each row by its signal:

| Signal | Finding when |
|---|---|
| `always-true-equality` | the equality serves an assertion or a comparison a test relies on. |
| `fake-success-exit` | it runs outside a CLI entry point or `main`, or ends a test run early. |
| `test-detection` | production behavior differs under test. |
| `hidden-call-state` | a pure-looking function's result depends on how often it ran. |
| `grader-patching` | a hook, report, `expect` or clock patch changes outcomes rather than isolating time. |
| `skipped-test`, `focused-test` | the change that breaks behavior adds the skip or focus. |

Not scanned; read the diff:

- One diff changes code and an expected literal, or flips polarity (`assertEqual` ↔ `assertNotEqual`).
- A branch on the test's literal input returns the expected constant.
- A scorer stubbed to full marks.
- A deleted or renamed test file: check-diff lists its checks under `missing`.

## CI and thresholds

New in the reviewed change → stop, escalate; a pre-existing setting with a stated reason (informational coverage job) is no finding.

- `coverage-threshold` rows: compare each value with the base revision; a lowered threshold is the finding.
- `ignored-failure` rows (`continue-on-error`, `allow_failure`, `|| true`, `passWithNoTests`): a check whose failure no longer fails the job.
- Not scanned: a new `if:` on a job, a removed `pull_request` trigger.

## Weak oracles

A coverage gap, not dead weight; with known intended behavior, strengthen per [Replacement tests](#replacement-tests).

- Never failed: a test added with a fix passes on the pre-change code. Run it in a separate worktree or checkout at the base revision, never by reverting, stashing or checking out in a shared tree; if it passes there, it does not test the fix. Behavior-preserving refactors are the exception: their characterization tests pass on both by design.
- No assertion: calls code, checks nothing.
- `existence-only-assertion`, `boolean-only-assertion`, `call-only-assertion`, `snapshot-only-assertion` rows: a weak oracle when it is the test's only check of the behavior; a bare `assertTrue(` on a computed flag needs the same reading.
- Untested new `catch`/`except` → report unproven, with coverage evidence; delete as unreachable only with caller proof.

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

- Assert the outcome, not the call: a `db.exec` spy count passes with a wrong schema → query `sqlite_master` on `new DatabaseSync(':memory:')`.
- Prompt or doc text: check real parser fields, links and declared limits; evaluate behavior with representative requests when meaning changes. A full-sentence or keyword regex assertion does not prove instruction compliance.
- Isolate and control time, env, cwd, fixtures: explicit `env`/`cwd` for `spawnSync`; `mkdtempSync` dirs removed in `afterEach`; capture each `process.env` value before overwriting, restore or delete in teardown; no shared state; declare platform or service dependencies.
- Name by outcome: `it('test case 3')` → `it('returns 404 when the key is missing')`. Follow the project's domain naming instead of temporary iteration names.
- Minimal fixtures; failures name expected and actual.
- Compare coverage before and after with [coverage-diff](../scripts/coverage-diff.mjs): `node <skill-folder>/scripts/coverage-diff.mjs <before> <after>` (lcov, istanbul or coverage.py JSON; add `--before-root <checkout>` when the baseline ran in another checkout). Exit 1 lists `lost` files with more uncovered lines or branches; a moved file shows as `removed` plus `lost`, so compare those two entries. Compare behavioral cases by reading both tests; restore losses via a public contract.
