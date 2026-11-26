Load when a change edits tests, fixtures, graders, CI, or thresholds, or production code reacts to being tested. Removing test debt or writing a replacement test: [test debt](test-debt.md).

# Tests

## Gaming

When checks change with the code they check, compare the diff with the owning contract. Escalate confirmed weakening via the [escalation protocol](agent-blockers.md#escalation-protocol).

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

A coverage gap, not dead weight; with known intended behavior, strengthen per [Replacement tests](test-debt.md#replacement-tests).

- Never failed: a test added with a fix passes on the pre-change code. Run it in a separate worktree or checkout at the base revision, never by reverting, stashing or checking out in a shared tree; if it passes there, it does not test the fix. Behavior-preserving refactors are the exception: their characterization tests pass on both by design.
- No assertion: calls code, checks nothing.
- `existence-only-assertion`, `boolean-only-assertion`, `call-only-assertion`, `snapshot-only-assertion` rows: a weak oracle when it is the test's only check of the behavior; a bare `assertTrue(` on a computed flag needs the same reading.
- Untested new `catch`/`except` → report unproven, with coverage evidence; delete as unreachable only with caller proof.
