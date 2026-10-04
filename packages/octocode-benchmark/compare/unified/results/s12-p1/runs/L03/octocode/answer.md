Callbacks are stored as tuples on the connection, dropped on savepoint rollback, connection close or a rollback, and run once the outermost transaction has committed and autocommit is switched back on. All paths below are under `django/`.

**Storing**
- `transaction.on_commit(func, using=None, robust=False)` (`db/transaction.py:142-147`) calls `get_connection(using).on_commit(func, robust)`.
- `BaseDatabaseWrapper.on_commit` (`db/backends/base/base.py:727`) first raises `TypeError` if `func` isn't callable.
- Inside an atomic block (`in_atomic_block`), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`base.py:730-732`). The list is initialised at `base.py:102`.
- Outside a transaction in autocommit mode, it runs `func` immediately (`base.py:737-747`). With `robust=True`, exceptions are caught and logged.
- In manual transaction management (not in an atomic block, autocommit off), it raises `TransactionManagementError` (`base.py:733-736`).

**Discarding**
- `savepoint_rollback(sid)` rebuilds the list without any entry whose savepoint set contains `sid` (`base.py:415-420`). Only callbacks registered while that savepoint was active are removed.
- `rollback()` sets `self.run_on_commit = []` (`base.py:341`). `close()` does the same (`base.py:347`). So does `connect()` (`base.py:261`).
- `Atomic.__exit__` calls `connection.rollback()` or `savepoint_rollback` on failure (`db/transaction.py:263,279,296,308`). Those lines are only grep hits, and I did not read the surrounding code.

**Running**
- A successful `commit()` sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:330`). The flag is initialised to `False` at `base.py:106`.
- When the outermost `Atomic.__exit__` calls `connection.set_autocommit(True)` (`db/transaction.py:320`), `set_autocommit` sees the flag (`base.py:490-492`). It then calls `run_and_clear_commit_hooks()` and resets the flag to `False`.
- `run_and_clear_commit_hooks` (`base.py:749-766`) first calls `validate_no_atomic_block()`. It then swaps `self.run_on_commit` for a fresh list and pops callbacks from the old one in registration order. A callback that registers another `on_commit` therefore runs it immediately, since the transaction has already ended and the connection is in autocommit mode. If `robust` is true, an exception is logged and the remaining callbacks still run. Otherwise the exception propagates and the rest of the swapped-out list is lost.

**Uncertainty:** I did not read `Atomic.__enter__`/`__exit__` in full (`db/transaction.py:~229-320`). The link from the outermost exit to `commit()` and then `set_autocommit(True)` is inferred from the grep hits at lines 276 and 320.