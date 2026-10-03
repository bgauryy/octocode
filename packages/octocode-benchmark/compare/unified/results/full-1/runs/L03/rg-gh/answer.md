**Summary:** Callbacks sit in a per-connection list, `connection.run_on_commit`. Rolling back a savepoint, a full rollback, or closing the connection drops them. They run after the outermost `atomic` block commits and autocommit is switched back on. All line numbers below are at 4fab678a07.

**Registering**
- `transaction.on_commit(func, using=None, robust=False)` at `django/db/transaction.py:142-147` calls `get_connection(using).on_commit(func, robust)`.
- `BaseDatabaseWrapper.on_commit` (`django/db/backends/base/base.py:727`) first raises `TypeError` if `func` isn't callable.
- **Inside an atomic block** (`in_atomic_block` true), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`base.py:732`). The set records which savepoints were active at registration.
- **Manual transaction management** (not in an atomic block, autocommit off): it raises `TransactionManagementError` (`base.py:733-736`).
- **Autocommit with no transaction:** it runs `func` immediately (`base.py:737-748`). With `robust=True`, an exception is caught and logged with `logger.exception` instead of propagating.
- The list is initialised to `[]` at `base.py:102`.

**Discarding**
- **Savepoint rollback:** `savepoint_rollback(sid)` (`base.py:405`) rebuilds the list at `base.py:416-418`. It keeps only entries whose `sids` set does not contain the rolled-back `sid`. This is how a failed inner `atomic()` discards the callbacks registered inside it.
- **Full rollback:** `base.py:341` resets the list to `[]` (in the rollback method just above the `needs_rollback = False` line, which I didn't view in full).
- **Close:** `base.py:347` resets the list to `[]` in `close()`.
- **New connection:** `base.py:261` resets it after `connect()`.

**Running**
1. The outermost `Atomic.__exit__` (`transaction.py:236`) calls `connection.commit()`. That call is in the "Commit transaction" branch, reached when there is no exception and no `needs_rollback`.
2. `commit()` (`base.py:~322-330`) calls `_commit()`, then sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:330`).
3. On leaving the outermost block, `Atomic.__exit__` restores autocommit. I saw the `finally` block start at `transaction.py:312` but not the `set_autocommit(True)` call itself, so that step is inferred.
4. In `set_autocommit` (`base.py:490-492`), if `autocommit and self.run_commit_hooks_on_set_autocommit_on`, it calls `run_and_clear_commit_hooks()` and resets the flag to `False`.
5. `run_and_clear_commit_hooks` (`base.py:749-766`) does the following:
   - It first calls `validate_no_atomic_block()`.
   - It swaps `self.run_on_commit` for a fresh `[]`, so callbacks registered while the hooks run go into the new list.
   - It pops entries from the old list in registration order (FIFO).
   - It calls each `func()`.
   - If `robust` is set, exceptions are logged and swallowed. Otherwise the exception propagates, and the callbacks not yet run are lost because the list was already swapped.

**Uncertainty:** I did not view the `set_autocommit(True)` call in `Atomic.__exit__`'s `finally` block, or the exact line of the rollback-path reset.