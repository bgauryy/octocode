Callbacks are stored on the connection while an atomic block is open, dropped on rollback, and run right after the outermost commit. All line numbers below are in `django/db/backends/base/base.py` unless another file is named.

**Storing**
- `transaction.on_commit(func, using=None, robust=False)` (`django/db/transaction.py:142`) calls `get_connection(using).on_commit(func, robust)`.
- `BaseDatabaseWrapper.on_commit` (`:727`) raises `TypeError` if `func` isn't callable.
- Inside an atomic block (`in_atomic_block`), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`:732`). The set records which savepoints were active at registration time.
- Outside an atomic block:
  - In manual transaction management (autocommit off), it raises `TransactionManagementError`.
  - In autocommit mode, it runs `func` immediately. With `robust=True`, exceptions are caught and logged with `logger.exception`.
- `run_on_commit` starts as `[]` (`:102`).

**Discarding**
- `savepoint_rollback(sid)` (`:405`) rebuilds the list without any entry whose `sids` contains `sid`. This drops callbacks registered while that savepoint was active, including in nested savepoints.
- `rollback()` sets `self.run_on_commit = []` (`:341`).
- `close()` also sets it to `[]` (`:347`).
- `connect()` resets it too (`:261`).

**Running**
- `commit()` sets `run_commit_hooks_on_set_autocommit_on = True` (`:330`). I didn't read the outer `atomic` code that calls it.
- When autocommit is turned back on, `set_autocommit` (`:490`) sees the flag and calls `run_and_clear_commit_hooks()`, then resets the flag. This is why hooks fire only after the outermost atomic block commits and autocommit is restored.
- `run_and_clear_commit_hooks` (`:749`):
  - It calls `validate_no_atomic_block()`.
  - It swaps `self.run_on_commit` for a fresh list, so callbacks registered by a running callback go on the new list.
  - It then pops entries from the front of the old list, so they run in registration order.
  - With `robust=True`, an exception is logged and later callbacks still run. Otherwise the exception propagates and the remaining callbacks in that batch are not run.

**Uncertainty:** I didn't read the `atomic.__exit__` code that triggers `commit()` and the autocommit restore. That link is inferred from the `set_autocommit` and `commit` lines above.