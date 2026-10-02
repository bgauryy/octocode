**1. Helped:**
- The first `localSearch` (regex `run_on_commit|def on_commit|run_and_clear_commit_hooks` over `django/db`) located nearly every relevant line in one call, and its `next.read` hint pointed at the right file.
- The multi-range `localFetch` on `base.py` returned the exact bodies of `on_commit`, `run_and_clear_commit_hooks` and `savepoint_rollback`, with line numbers.
- The follow-up `localSearch` for `run_commit_hooks_on_set_autocommit_on` showed how commit and autocommit connect.

**2. Did not help:**
- My `localFetch` on `transaction.py` failed validation because I wrote `" 140-150"` with a leading space. That was my error, and I never retried it.
- Omitted-line gaps in the `base.py` fetch hid the enclosing `def` lines. The attribution of line 261 to `connect()` and line 341 to `rollback()` is therefore inferred, not seen.
- The `transaction.py` search returned only isolated matches. I never read `Atomic.__enter__` or `__exit__`.

**3. Next time:**
- Re-run the corrected `localFetch` on `transaction.py` for roughly 200-330.
- Use `lspSearch` or `astSearch` to confirm the enclosing function names.
- Check the thread/async and test-case (`TestCase.captureOnCommitCallbacks`) paths if relevant.

**4. Confidence:** medium-high.
- The storage, savepoint-discard and run-loop claims are directly backed by the lines I read.
- The claim that the outermost `atomic` exit reaches `set_autocommit(True)` rests on a grep hit at `transaction.py:320`, not a full read.
- I flagged this in the answer.