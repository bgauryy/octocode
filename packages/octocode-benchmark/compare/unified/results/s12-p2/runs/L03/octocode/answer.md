Callbacks are kept as tuples on the connection, dropped on rollback, and run once the outermost transaction commits and autocommit comes back on. I didn't read the `finally` clause of `Atomic.__exit__`, so the step that re-enables autocommit there is inferred from the `set_autocommit` code in `base.py`.

**Storing**
- `transaction.on_commit(func, using=None, robust=False)` only forwards to `get_connection(using).on_commit(func, robust)` (`django/db/transaction.py:142-147`).
- `BaseDatabaseWrapper.on_commit` (`django/db/backends/base/base.py:727-747`) first raises `TypeError` if `func` isn't callable.
- Inside an atomic block (`in_atomic_block`), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`base.py:730-732`). The set of savepoint ids records which savepoints were active at registration.
- With no atomic block and autocommit on, it runs `func()` immediately. With `robust=True`, exceptions are caught and logged instead of raised (`base.py:737-747`).
- With no atomic block and autocommit off (manual transaction management), it raises `TransactionManagementError` (`base.py:733-736`).
- `run_on_commit` starts as `[]` in `__init__` (`base.py:102`) and again in `connect()` (`base.py:261`).
- `Atomic.__enter__` pushes a savepoint id, or `None`, onto `connection.savepoint_ids` for nested blocks (`transaction.py:218-227`). That is where the ids stored with each callback come from.

**Discarding**
- **Savepoint rollback:** `savepoint_rollback(sid)` rebuilds the list without any entry whose id set contains `sid` (`base.py:415-420`). Callbacks registered inside a rolled-back inner `atomic` are dropped, and `Atomic.__exit__` calls this at `transaction.py:296`.
- **Full rollback:** `rollback()` sets `run_on_commit = []` (`base.py:341`).
- **Closing the connection:** `close()` also sets `run_on_commit = []` (`base.py:347`).
- **Inner block exits that don't roll back:** if the inner block has no savepoint (`sid` is `None`), the exit only sets `needs_rollback = True`, so the whole transaction rolls back at the outer level (`transaction.py:292-293`). If the inner block exits cleanly, `savepoint_commit` releases the savepoint and the callbacks stay in the list. I didn't read `savepoint_commit`'s body (lines 426-483 of `base.py`), so I'm not claiming anything about what it does to them.

**Running**
- When the outermost `atomic` exits cleanly, `Atomic.__exit__` calls `connection.commit()` (`transaction.py:274-276`).
- `commit()` does `_commit()` and then sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:327-330`). It does not run the hooks itself.
- When autocommit is turned back on, `set_autocommit` sees `autocommit and run_commit_hooks_on_set_autocommit_on`. It calls `run_and_clear_commit_hooks()` and resets the flag (`base.py:490-492`).
- `run_and_clear_commit_hooks` (`base.py:749-766`) first calls `validate_no_atomic_block()`. It then swaps `run_on_commit` for a fresh `[]` and pops entries from the old list in registration order (FIFO).
- A callback that registers another `on_commit` therefore appends to the new list. Because the atomic block has already exited, that nested call runs immediately via the autocommit branch.
- If a callback has `robust=True`, exceptions are logged and the rest still run. Otherwise the exception propagates and the remaining callbacks in that batch are not run, because they were already popped off the live list.

**Uncertainty**
- I didn't read the `finally` clause of `Atomic.__exit__` (after line 312), so the autocommit re-enable is inferred, as noted at the top.
- The test helper `captureOnCommitCallbacks` (`django/test/testcases.py:1514`) also reads `run_on_commit`. I didn't cover how it uses it.