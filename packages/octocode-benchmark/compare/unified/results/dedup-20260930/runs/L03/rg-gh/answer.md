Callbacks are stored per connection in a list. They are discarded on rollback, on savepoint rollback, on connect and on close. They run after the outermost transaction commits, when autocommit is switched back on. All line numbers below are in `django/db/backends/base/base.py` unless stated otherwise.

**Storage**
- `transaction.on_commit(func, using=None, robust=False)` only delegates to `get_connection(using).on_commit(func, robust)` (`django/db/transaction.py:142-147`).
- Each connection has a list `self.run_on_commit = []` (`:102`).
- `BaseDatabaseWrapper.on_commit` (`:727`) raises `TypeError` if `func` isn't callable.
- If `in_atomic_block` is true, it appends the tuple `(set(self.savepoint_ids), func, robust)` to `run_on_commit` (`:732`). The set records which savepoints were active when the callback was registered.
- `Atomic.__enter__` pushes a savepoint id onto `connection.savepoint_ids`, or `None` if no savepoint is made (`django/db/transaction.py:222-227`).
- Outside an atomic block:
  - In manual transaction management (autocommit off), it raises `TransactionManagementError` (`:733-736`).
  - In autocommit mode, it runs `func` immediately (`:737-747`). With `robust=True`, exceptions are logged and swallowed.

**Discarding**
- **Savepoint rollback:** `savepoint_rollback(sid)` rebuilds the list and keeps only entries where `sid not in sids` (`:416-418`). Callbacks registered inside a rolled-back inner `atomic` are therefore dropped.
- **Full rollback:** `rollback()` sets `run_on_commit = []` (`:341`).
- **Close:** `close()` also clears the list (`:347`).
- **Connect:** `connect()` resets it to `[]` (`:261`).

**Running**
- `commit()` calls `_commit()` and sets `run_commit_hooks_on_set_autocommit_on = True` (`:330`). It does not run the hooks itself.
- When the outermost `Atomic.__exit__` finishes, it calls `connection.set_autocommit(True)` (`django/db/transaction.py` finally block, about `:317-318`).
- In `set_autocommit`, if autocommit is now on and the flag is set, it calls `run_and_clear_commit_hooks()` and resets the flag (`:490-492`).
- `run_and_clear_commit_hooks` (`:749`) first calls `validate_no_atomic_block()`. It then swaps `run_on_commit` for a fresh `[]` and pops entries from the front of the old list in registration order (FIFO).
- A non-robust callback that raises propagates the exception, and the remaining callbacks in that batch are lost. A `robust=True` callback has its exception logged via `logger.exception` and the rest still run (`:755-766`).
- Callbacks that register new `on_commit` hooks while running go onto the fresh list. They run immediately, because the connection is back in autocommit mode.

**Uncertainty**
- I read the `__exit__` code for the commit path only in part. The `set_autocommit(True)` call is in the `finally` block, but I didn't see the exact line number, so `:317-318` is approximate.
- The `rollback()` and `close()` line numbers come from a line-ranged read and I didn't re-check them.