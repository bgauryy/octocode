**Short answer:** Django keeps on-commit callbacks in a per-connection list, `connection.run_on_commit`. Rolling back drops them. They run only after the outermost transaction has committed and autocommit is switched back on.

**Entry point**
- `transaction.on_commit(func, using=None, robust=False)` just calls `get_connection(using).on_commit(func, robust)` (`django/db/transaction.py:142-147`).

**Storage** (`django/db/backends/base/base.py`)
- `self.run_on_commit = []` is initialised in `__init__` at line 102. It holds tuples of `(set_of_savepoint_ids, func, robust)`.
- `on_commit()` (lines 727-747) first raises `TypeError` if `func` isn't callable.
- If `in_atomic_block` is true, it appends `(set(self.savepoint_ids), func, robust)` (line 732). The savepoint IDs active at registration are recorded with the callback.
- If it is not in an atomic block and autocommit is off, it raises `TransactionManagementError` (lines 733-736).
- If it is not in an atomic block and autocommit is on, it runs `func` immediately (lines 737-747). With `robust=True`, exceptions are caught and logged instead of raised.

**Discarding**
- `savepoint_rollback(sid)` rebuilds the list without any entry whose saved set contains `sid` (lines 416-420). This discards callbacks registered while that savepoint was active, which is what happens when an inner `atomic()` block fails.
- `rollback()` sets `run_on_commit = []` (line 341). `close()` does the same (line 347), and so does `connect()` (line 261).
- `Atomic.__exit__` calls `connection.savepoint_rollback(sid)` or `connection.rollback()` on its failure paths (`transaction.py:263, 296, 308`), so those reach the discard logic above.

**Running**
- A successful `commit()` sets `run_commit_hooks_on_set_autocommit_on = True` (line 330). It does not run the hooks itself.
- `set_autocommit(True)` then checks `if autocommit and self.run_commit_hooks_on_set_autocommit_on` (line 490). It calls `run_and_clear_commit_hooks()` and resets the flag to `False` (lines 491-492).
- For an outermost `atomic()`, `Atomic.__exit__` calls `connection.commit()` (`transaction.py:276`) and later `connection.set_autocommit(True)` (`transaction.py:320`). That second call is what fires the hooks.
- `run_and_clear_commit_hooks()` (lines 749-767) first calls `validate_no_atomic_block()`. It then swaps the list for a fresh `[]` and pops callbacks in registration order (FIFO), ignoring the savepoint IDs.
- A callback that fails with `robust=True` is logged via `logger.exception`, and the remaining callbacks still run. A failure with `robust=False` propagates and stops the loop.
- Callbacks that were popped before the failure have already run. The ones still in the local list are lost, because `self.run_on_commit` was already reset.
- A callback that registers another `on_commit` goes into the new list. Since the atomic block has exited, that call hits the "run immediately" branch.

**Uncertainty:** I didn't read `Atomic.__exit__` line by line, so the call sites cited from `transaction.py` come from grep matches and not from reading the surrounding control flow.