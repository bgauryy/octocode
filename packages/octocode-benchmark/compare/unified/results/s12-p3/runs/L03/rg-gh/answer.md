Callbacks live in a per-connection list. They are dropped on rollback, on savepoint rollback, and on close. They run once the outermost transaction commits, or immediately if no transaction is open. All line numbers are in `django/db/` at 4fab678a07.

**Storage**
- `transaction.on_commit(func, using=None, robust=False)` just calls `get_connection(using).on_commit(func, robust)` (`transaction.py:142-147`).
- `BaseDatabaseWrapper.run_on_commit` is a list of `(set_of_savepoint_ids, func, robust)` tuples. It is initialised to `[]` at `backends/base/base.py:102`.
- `BaseDatabaseWrapper.on_commit` (`backends/base/base.py:727`) first raises `TypeError` if `func` isn't callable. After that it branches three ways:
  - **Inside an atomic block:** it appends `(set(self.savepoint_ids), func, robust)` to `run_on_commit` (line 732). The snapshot of savepoint ids records which savepoints were active at registration.
  - **Not in an atomic block and autocommit is off (manual transaction management):** it raises `TransactionManagementError` (lines 733-736).
  - **Not in an atomic block and autocommit is on:** it calls `func()` immediately. With `robust=True`, exceptions are caught and logged with `logger.exception` (lines 737-746).

**Discarding**
- `savepoint_rollback(sid)` rebuilds the list without any entry whose `sids` contains that savepoint (`base.py:416-418`). This removes callbacks registered while that savepoint was active.
- `rollback()` sets `run_on_commit = []` (`base.py:341`).
- `close()` also clears the list (`base.py:347`).
- `connect()` resets it too (`base.py:261`).

**Running**
- `commit()` calls `_commit()` and then sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:322-330`). It does not run the hooks itself.
- When the outermost `Atomic.__exit__` finishes, its `finally` block calls `connection.set_autocommit(True)` (`transaction.py:~318-322`).
- `set_autocommit` then checks `if autocommit and self.run_commit_hooks_on_set_autocommit_on`. If so, it calls `run_and_clear_commit_hooks()` and resets the flag to `False` (`base.py:490-492`).
- `run_and_clear_commit_hooks` (`base.py:749-765`):
  - It first asserts no atomic block is active.
  - It swaps `run_on_commit` for a fresh `[]`.
  - It then pops the old entries in registration order (FIFO) and calls each one.
  - A `robust` callback has its exception logged and the loop continues.
  - A non-robust callback's exception propagates, and the remaining callbacks in that batch are not run, because they were already removed from the connection's list.
  - A callback that itself calls `on_commit` adds to the new list. That runs immediately if the connection is in autocommit and no atomic block is open.

**Not verified:** I didn't read the `Atomic.__enter__` code or the tests, so I haven't confirmed how `savepoint_ids` is populated.