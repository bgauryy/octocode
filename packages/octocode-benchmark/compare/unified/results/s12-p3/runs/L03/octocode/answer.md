**Summary:** `transaction.on_commit` hands the callback to the connection. Inside an `atomic` block the connection stores it in a list. A savepoint rollback or a full rollback removes it. The outermost commit runs it, but only once autocommit is switched back on. All line numbers below are at 4fab678a07.

**Storage**
- `django/db/transaction.py:142-147`: `on_commit(func, using=None, robust=False)` calls `get_connection(using).on_commit(func, robust)`.
- `django/db/backends/base/base.py:727-732`: `BaseDatabaseWrapper.on_commit` raises `TypeError` if `func` isn't callable. If `in_atomic_block` is true, it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit`.
- `base.py:98-102`: `self.run_on_commit` is initialised as a list of `(sids, func, robust)` tuples. `sids` is the set of savepoint IDs active at registration.
- If there is no atomic block and autocommit is on, the callback runs immediately (`base.py:737-747`). With `robust=True`, an exception is logged instead of raised.
- If there is no atomic block and autocommit is off (manual transaction management), it raises `TransactionManagementError` (`base.py:733-736`).

**Discarding**
- Savepoint rollback (`base.py:413-420`): after `_savepoint_rollback(sid)`, the list is filtered to keep only entries where `sid not in sids`. Callbacks registered while that savepoint was active are dropped.
- Full rollback (`base.py:341`): `rollback()` sets `run_on_commit = []`.
- `close()` (`base.py:347`) and `connect()` (`base.py:261`) also reset the list to `[]`.
- `Atomic.__exit__` calls `connection.rollback()` for the outermost block on an exception or `needs_rollback` (`transaction.py:305-312`). A nested block uses `savepoint_rollback` instead (`transaction.py:292-299`).

**Running**
1. The outermost `Atomic.__exit__` calls `connection.commit()` (`transaction.py:273-276`). `commit()` (`base.py:323-330`) runs `_commit()` and sets `run_commit_hooks_on_set_autocommit_on = True`. It does not run the hooks itself.
2. The `finally` in `__exit__` then calls `connection.set_autocommit(True)` (`transaction.py:315-320`). In `set_autocommit` (`base.py:490-492`), if autocommit is True and that flag is set, it calls `run_and_clear_commit_hooks()` and resets the flag to False.
3. `run_and_clear_commit_hooks` (`base.py:749-766`) first calls `validate_no_atomic_block()`. It then swaps `self.run_on_commit` for an empty list and pops entries from the front of the old list, so they run in registration order. A non-robust `func()` that raises propagates, and the remaining callbacks are not run. A `robust` callback has its exception caught and logged with `logger.exception`, and the loop continues.
4. Because the list is swapped out before the loop, a callback that registers a new `on_commit` runs it immediately: the connection is already in autocommit with no atomic block, so the immediate-run path applies.

**Uncertainty:** I didn't read the `savepoint_commit` / `savepoint_rollback` bodies beyond `base.py:413-420`. I also didn't check test-case behaviour such as `captureOnCommitCallbacks`.