Load when a change edits tests, fixtures, graders, CI, or thresholds; production code reacts to being tested; or test debt is removed or replaced.

# Tests

## Gaming

When checks change with the code they check, compare the diff with the owning contract. The signals below need investigation; escalate confirmed weakening via the [escalation protocol](agent-defects.md#escalation-protocol).

- One diff changes code and an expected literal, or flips polarity (`assertEqual` ↔ `assertNotEqual`).
- A branch on the test's literal input returns the expected constant.
- Always-true equality: `__eq__` → `True`, `equals` → `true`, `fn eq` → `true`.
- Fake-success exit outside a CLI entrypoint or `main`: `sys.exit(0)`, `os._exit(0)`, `process.exit(0)`, `pytest.skip(`, `raise SkipTest`.
- Test detection in production: `'pytest' in sys.modules`, `PYTEST_CURRENT_TEST`, `JEST_WORKER_ID`, `NODE_ENV === 'test'`, `inspect.stack(`, `sys._getframe`, `new Error().stack`.
- Hidden call state: module-level `_call_count`/`_calls`, `global`/`nonlocal` counters, mutable statics in pure-looking functions.
- Framework or grader patching: `conftest.py` hooks rewriting outcomes (`pytest_runtest_makereport`, `TestReport`); `expect` redefined in setup; patched `time.time`, `perf_counter`, `Date.now`; scorer stubbed to full marks.
- The breaking change adds `it.skip`, `xit`, `@pytest.mark.skip`, or deletes a test file.

## CI and thresholds

New in the reviewed change → stop, escalate; a pre-existing setting with a stated reason (informational coverage job) is no finding.

- Lowered `coverageThreshold`, `--cov-fail-under`, `fail_under`.
- Ignored failure: `continue-on-error: true`, `|| true` after a check.
- New `if:` on a job; `--passWithNoTests`; removed `pull_request` trigger; renamed or deleted test files.

## Weak oracles

A coverage gap, not dead weight; with known intended behavior, strengthen per [Replacement tests](#replacement-tests).

- No assertion: calls code, checks nothing.
- Existence only: `toBeDefined`, `toBeTruthy`, `not.toBeNull`, `assertIsNotNone`, `is not None`.
- Boolean only: `assert True`, `expect(true)`, bare `assertTrue(` on a computed flag.
- Call only: `toHaveBeenCalled()`/`assert_called()` without argument or result check.
- Snapshot only: `toMatchSnapshot`, no behavioral assertion.
- Untested new `catch`/`except` → report unproven, with coverage evidence; delete as unreachable only with caller proof.

## Test debt

Prove by behavioral overlap and coverage after reading both tests; "the suite still passes" proves nothing. Signal → evidence before delete:

- Iteration file: name ends `-N`/`-N-N`, dated, or version-slugged → every describe block is a strict behavioral subset of an existing base.
- Date-pinned bug test: one dated incident → a domain test covers it.
- Skipped test: `it.skip`, `describe.skip`, `xit`, `xdescribe`, `test.skip`, `it.todo` → still required? Restore, replace, or document regression intent (a missing ticket proves nothing).
- Rigid mock: rebuilds a large internal object to check one field, counts internal-helper calls, spies on an unexported symbol → outcome assertable via public API; repair if the behavior matters.
- Mirror mock: `__mocks__` duplicating a production module's shape → real implementation runs in a temp directory.
- Redundant stub: `mockReturnValue` matching the real return, no `expect` on the mock → decorative.
- Unused setup: `beforeEach`/`afterEach` state no `it` in the block touches.
- Environment coupling: `spawnSync` without explicit `env:`, unreset `process.env` reads, cwd dependence → passes with the variable set and unset.

Excision: list candidates, diff each suffixed file against its base; keep blocks exercising paths the base misses; delete in one batch, run the package's tests; replace lost coverage or revise the delete; never relax acceptance or widen a mock to pass.

## Replacement tests

- Assert the outcome, not the call: a `db.exec` spy count passes with a wrong schema → query `sqlite_master` on `new DatabaseSync(':memory:')`.
- Prompt or doc text: check real parser fields, links and declared limits; evaluate behavior with representative requests when meaning changes. A full-sentence or keyword regex assertion does not prove instruction compliance.
- Isolate and control time, env, cwd, fixtures: explicit `env`/`cwd` for `spawnSync`; `mkdtempSync` dirs removed in `afterEach`; capture each `process.env` value before overwriting, restore or delete in teardown; no shared state; declare platform or service dependencies.
- Name by outcome: `it('test case 3')` → `it('returns 404 when the key is missing')`; files `<domain>-<what-is-under-test>.test.ts`, no iteration number or date.
- Minimal fixtures; failures name expected and actual.
- Compare coverage and behavioral cases before and after; restore losses via a public contract.
