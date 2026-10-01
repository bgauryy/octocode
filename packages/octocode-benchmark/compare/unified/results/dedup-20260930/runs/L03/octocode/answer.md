Callbacks registered with `transaction.on_commit` are kept in a per-connection list, dropped on savepoint rollback, full rollback or close, and run when the outermost transaction commits and autocommit is switched back on. Line numbers are from the pinned commit. I did not read `Atomic.__exit__`, so the link between the outermost atomic block exiting and `set_autocommit(True)` is inferred from the flag logic, not seen directly.

**Storage**
- `django/db/transaction.py:142` defines `on_commit(func, using=None, robust=False)`. It just calls `get_connection(using).on_commit(func, robust)`.
- `BaseDatabaseWrapper.on_commit` is at `django/db/backends/base/base.py:727`. It raises `TypeError` if `func` is not callable.
- Inside an atomic block (`in_atomic_block`), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (line 732). The list is initialised at line 102, and the tuple format is documented in the comment at lines 98-101.
- Outside a transaction, in autocommit mode, it runs `func()` immediately. With `robust=True`, exceptions are caught and logged.
- In manual transaction management (not in an atomic block, autocommit off), it raises `TransactionManagementError`.

**Discarding**
- **Savepoint rollback:** `savepoint_rollback` (around lines 414-419) rebuilds the list, keeping only entries whose saved savepoint-ID set does not contain the rolled-back `sid`. This drops callbacks registered while that savepoint was active.
- **Full rollback:** `rollback()` sets `self.run_on_commit = []` (line 341).
- **Close:** `close()` sets `self.run_on_commit = []` (line 347).
- **Reconnect:** `connect()` also resets the list (line 261).

**Running**
- `commit()` does not run the callbacks itself. After `_commit()` it sets `run_commit_hooks_on_set_autocommit_on = True` (line 330).
- In `set_autocommit` (lines 490-492), if `autocommit` is true and that flag is set, it calls `run_and_clear_commit_hooks()` and resets the flag to `False`.
- `run_and_clear_commit_hooks` (line 749) first calls `validate_no_atomic_block()`. It then swaps `self.run_on_commit` for an empty list, so callbacks registered while the hooks run go into the new list. It pops the old entries in registration order (FIFO) and calls each one.
- If `robust` is true, an exception is logged with `logger.exception` and the remaining callbacks still run. Otherwise the exception propagates and the callbacks still queued in the local list are lost.