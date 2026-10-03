# Test and Grader Gaming

Load when one change touches tests, fixtures, graders, CI, or thresholds together with the code they check, or when production code reacts to being tested. A gamed check invalidates every later verification, including VERIFY.

Everything here is report-only. Removing the trick exposes a real failure, so escalate through `references/agentic-correctness.md` § Escalation protocol and keep it out of every excision batch. Read the diff, never the agent's summary of it: completion reports have claimed passing tests that never ran.

## Weak oracles

| Class | Signal |
|---|---|
| No assertion | test body calls code and checks nothing |
| Existence only | `toBeDefined`, `toBeTruthy`, `not.toBeNull`, `assertIsNotNone`, `is not None` as the only check |
| Boolean only | `assert True`, `expect(true)`, a bare `assertTrue(` on a computed flag |
| Call only | `toHaveBeenCalled()` or `assert_called()` with no argument or result check |
| Snapshot only | `toMatchSnapshot` with no behavioral assertion |

A weak oracle is a coverage gap, not dead weight. Strengthen it with `references/test-hygiene.md` (Replacement tests) once the intended behavior is known.

## Gaming the check

| Trick | Signal |
|---|---|
| Assertion edited with the implementation | one diff changes the code and an expected literal, or flips polarity (`assertEqual` ↔ `assertNotEqual`) |
| Special-cased input or hardcoded expected value | a branch on the test's literal input; a return of the expected constant |
| Always-true equality | `__eq__` returning `True`, `equals` returning `true`, `fn eq` returning `true` |
| Early exit faking success | `sys.exit(0)`, `os._exit(0)`, `process.exit(0)`, `pytest.skip(`, `raise SkipTest` in implementation code, outside a CLI entrypoint or `main` |
| Test-environment detection | `'pytest' in sys.modules`, `PYTEST_CURRENT_TEST`, `JEST_WORKER_ID`, `NODE_ENV === 'test'`, `inspect.stack(`, `sys._getframe`, `new Error().stack` in production code |
| Hidden call state | module-level `_call_count` or `_calls`, `global`/`nonlocal` counters, mutable statics in pure-looking functions |
| Framework or grader patching | `conftest.py` hooks that rewrite outcomes (`pytest_runtest_makereport`, `TestReport`); `expect` redefined in setup; patched `time.time`, `perf_counter`, or `Date.now`; a scorer stubbed to full marks |
| Test removed in the change that made it fail | new `it.skip`, `xit`, `@pytest.mark.skip`, or a deleted test file |

## CI and threshold weakening

Treat any weakening that the change under review introduces as a stop: a lowered `coverageThreshold`, `--cov-fail-under`, or `fail_under`; `continue-on-error: true`; `|| true` after a check; a new `if:` that skips a job; `--passWithNoTests`; a removed `pull_request` trigger; renamed or deleted test files. A pre-existing setting with a stated reason (an informational coverage job) is not a finding.

## Untested failure paths

A new `catch` or `except` block that no test executes is unproven, not dead. Report it with the coverage evidence. Delete it as unreachable only with caller proof (`references/agentic-defects.md` § Scope-creep leftovers).

Next: for the escalation protocol load `references/agentic-correctness.md`; for replacement tests load `references/test-hygiene.md` (Replacement tests); for prevalence load `references/agentic-defects.md` (Audit order).
