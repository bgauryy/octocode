Callbacks are stored per connection, discarded on rollback, and run right after the outermost transaction commits. All paths below are under `django/db/` at 4fab678a07. I read the code but did not run it.

**Storage**
- `transaction.on_commit(func, using=None, robust=False)` hands off to `get_connection(using).on_commit(func, robust)` (`transaction.py:142-147`).
- `BaseDatabaseWrapper.__init__` creates `self.run_on_commit = []` (`backends/base/base.py:102`).
- `BaseDatabaseWrapper.on_commit` (`base.py:727`) first raises `TypeError` if `func` isn't callable. Then it branches three ways:
  - **Inside an atomic block:** it appends the tuple `(set(self.savepoint_ids), func, robust)` to `run_on_commit` (`base.py:731-732`). The set is a snapshot of the savepoint IDs active at registration time.
  - **Not in an atomic block, autocommit off:** it raises `TransactionManagementError`, because `on_commit()` can't be used in manual transaction management (`base.py:733-736`).
  - **Not in an atomic block, autocommit on:** it calls `func()` immediately (`base.py:737-747`). With `robust=True`, an exception is caught and logged with `logger.exception` instead of propagating.

**Discarding**
- **Savepoint rollback:** `savepoint_rollback(sid)` rebuilds the list without any entry whose saved set contains `sid` (`base.py:415-419`). Only callbacks registered while that savepoint was active are dropped. This is how a failed inner `atomic()` discards its own callbacks.
- **Full rollback:** `rollback()` ends with `self.run_on_commit = []` (`base.py:341`).
- **Close and reconnect:** `close()` clears the list (`base.py:347`), and so does `connect()` (`base.py:261`).
- **Outermost `atomic.__exit__`:** on an exception or `needs_rollback`, it calls `connection.rollback()` (`transaction.py:~305-311`). That reaches the full-rollback clear above.

**Running**
1. On a successful outermost commit, `atomic.__exit__` calls `connection.commit()`. `commit()` runs `_commit()` and then sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:323-330`).
2. In the `finally` of `atomic.__exit__`, the outermost block calls `connection.set_autocommit(True)` (`transaction.py:~316-321`).
3. `set_autocommit` ends with `if autocommit and self.run_commit_hooks_on_set_autocommit_on:`. It then calls `run_and_clear_commit_hooks()` and resets the flag to `False` (`base.py:490-492`).
4. `run_and_clear_commit_hooks` (`base.py:749-765`) does the following:
   - It calls `validate_no_atomic_block()`.
   - It swaps `run_on_commit` for an empty list, so callbacks registered by a running callback go onto the new list.
   - It pops and calls callbacks in registration order (FIFO).
   - With `robust=True`, it catches and logs exceptions from a callback and carries on with the rest.
   - With `robust=False`, an exception propagates out and the remaining callbacks in that batch are lost. They were already removed from `run_on_commit`, so they are not retried.

**Uncertainty**
- The `transaction.py` line numbers marked `~` come from a slightly offset sed range, so they are approximate. The code itself is as quoted.