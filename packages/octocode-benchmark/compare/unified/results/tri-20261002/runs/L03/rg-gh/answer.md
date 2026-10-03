**Summary:** `transaction.on_commit(func, using=None, robust=False)` is a thin wrapper (`django/db/transaction.py:142-147`). It calls `get_connection(using).on_commit(func, robust)`. All the logic is in `BaseDatabaseWrapper` in `django/db/backends/base/base.py`. I did not check the git HEAD: the `git rev-parse` call failed because `/dev/null` was blocked, so I'm assuming the checkout is at the pinned commit as stated.

**Storage** (`base.py:727-747`, `on_commit`)
- The callback is first checked with `callable()`, and a `TypeError` is raised if it isn't (`base.py:729`).
- If `in_atomic_block` is true, it appends the tuple `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (`base.py:732`). The set is a snapshot of the active savepoint IDs.
- `run_on_commit` is initialised to `[]` at `base.py:102`. It is also reset at `base.py:261`.
- If there is no atomic block and autocommit is off, it raises `TransactionManagementError` (`base.py:733-736`).
- If there is no atomic block and autocommit is on, the callback runs immediately and is not stored. With `robust=True`, exceptions are caught and logged (`base.py:739-747`).

**Discard**
- **Savepoint rollback.** `savepoint_rollback(sid)` rebuilds `run_on_commit`, dropping every entry whose saved `sids` contains the rolled-back `sid` (`base.py:415-420`). `Atomic.__exit__` calls this when an inner atomic block with a savepoint exits with an exception (`transaction.py:263`, `:296`).
- **Full rollback.** `rollback()` sets `run_on_commit = []` (`base.py:341`).
- **Close.** `close()` also sets `run_on_commit = []` (`base.py:347`).
- **Inner block exit without error.** A savepoint release (`savepoint_commit`) does not discard callbacks. They stay queued until the outermost commit.
- **Outer block failure with no savepoint.** In `Atomic.__exit__`, the outermost block rolls back through `connection.rollback()` (`transaction.py:~310-316`), which clears the list.

**Running**
- `Atomic.__exit__` on the outermost block calls `connection.commit()` (`transaction.py:~299-303`). `commit()` does not run hooks itself. It sets `run_commit_hooks_on_set_autocommit_on = True` (`base.py:323-330`).
- The `finally` block then calls `connection.set_autocommit(True)` (`transaction.py:~319-324`).
- In `set_autocommit`, if autocommit is on and `run_commit_hooks_on_set_autocommit_on` is set, it calls `run_and_clear_commit_hooks()` and resets the flag (`base.py:490-492`).
- `run_and_clear_commit_hooks` (`base.py:749-767`) first checks `validate_no_atomic_block()`. It then swaps `run_on_commit` for a fresh `[]` and pops callbacks from the front of the old list (`pop(0)`), so they run in registration order.
- A callback registered while hooks are running goes onto the new list. Because `in_atomic_block` is false by then, it runs immediately (the autocommit branch above).
- With `robust=True`, an exception is logged via `logger.exception` and the remaining callbacks still run (`base.py:755-764`). Otherwise the exception propagates and the remaining callbacks in the old list are dropped (they were already removed from `run_on_commit`).

**Uncertainty:** the `transaction.py` line numbers marked `~` are approximate. I read that code in a combined output without line numbers, so I inferred them. The `base.py` lines come from the grep output and are reliable.