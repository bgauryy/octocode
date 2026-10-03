Callbacks registered with `transaction.on_commit` are stored in a per-connection list. They are discarded by savepoint rollback, by a full rollback, or when the connection is closed or reconnected. They run after the outermost commit, when autocommit is switched back on. Paths below are relative to `django/db/`.

**Storing**
- `transaction.on_commit(func, using=None, robust=False)` only delegates to `get_connection(using).on_commit(func, robust)` (`transaction.py:142-147`).
- `BaseDatabaseWrapper.on_commit` (`backends/base/base.py:727`) first raises `TypeError` if `func` isn't callable.
- Inside an atomic block, it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`base.py:730-732`). The set records which savepoints were active at registration time.
- Outside a transaction in autocommit mode, it calls `func()` immediately (`base.py:737-747`). With `robust=True`, exceptions are caught and logged.
- If autocommit is off and there is no atomic block, it raises `TransactionManagementError` (`base.py:733-736`).
- `run_on_commit` is initialised to `[]` in `__init__` (`base.py:102`).

**Discarding**
- `savepoint_rollback(sid)` rebuilds the list, keeping only entries where `sid not in sids` (`base.py:415-420`). Callbacks registered while that savepoint was active are therefore dropped. An inner `atomic` block that fails rolls back its savepoint through this path (`transaction.py:295-299`).
- `rollback()` resets `run_on_commit = []` (`base.py:341`).
- `close()` resets it (`base.py:347`).
- `connect()` resets it (`base.py:261`).

**Running**
- `Atomic.__exit__` calls `connection.commit()` only for the outermost block (`transaction.py:273-276`). Inner blocks just release their savepoint, so their callbacks stay queued.
- `commit()` sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:330`).
- When `Atomic.__exit__` then calls `connection.set_autocommit(True)` (`transaction.py:320`), `set_autocommit` sees that flag and calls `run_and_clear_commit_hooks()`. It then resets the flag to `False` (`base.py:490-492`). I found no other caller of `run_and_clear_commit_hooks` outside tests.
- `run_and_clear_commit_hooks` (`base.py:749-766`) first calls `validate_no_atomic_block()`. It then swaps `self.run_on_commit` for a fresh `[]` and pops callbacks from the old list in registration order (FIFO).
- With `robust=True`, an exception is logged and the remaining callbacks still run. Otherwise the exception propagates and the remaining callbacks in that batch are lost, since the list was already cleared.
- A callback that registers another `on_commit` callback appends to the new list. Because `set_autocommit` resets `run_commit_hooks_on_set_autocommit_on` only after the hooks return, a nested registration made while the connection is in autocommit mode, outside an atomic block, runs immediately.

I did not check how the test framework's `TestCase` handles these hooks, since it was outside the question.