**Summary:** `transaction.on_commit` stores a callback on the database connection. A savepoint rollback or a full rollback discards it. The outermost `atomic` exit runs it, after the commit.

All line numbers below are at 4fab678a07.

**Storing**
- `django/db/transaction.py:142-147`: `on_commit(func, using=None, robust=False)` calls `get_connection(using).on_commit(func, robust)`.
- `django/db/backends/base/base.py:727-747`: `BaseDatabaseWrapper.on_commit` first raises `TypeError` if `func` isn't callable.
  - **Inside an atomic block** (`in_atomic_block`), it appends `(set(self.savepoint_ids), func, robust)` to `self.run_on_commit` (line 732). The set of savepoint ids active at registration time is what makes selective discarding possible.
  - **Not in an atomic block and not in autocommit** (manual transaction management), it raises `TransactionManagementError` (733-736).
  - **Autocommit and no transaction**, it runs `func()` immediately (737-747). With `robust=True`, an exception is caught and logged with `logger.exception` instead of propagating.
- `run_on_commit` is initialised to `[]` at line 102.

**Discarding**
- **Savepoint rollback** (`base.py:405-420`): `savepoint_rollback(sid)` rebuilds the list, keeping only entries where `sid not in sids` (416-420). Callbacks registered while that savepoint was active are dropped.
- **Full rollback** (`base.py:341`): `rollback()` sets `run_on_commit = []`.
- **Close** (`base.py:347`): `close()` sets `run_on_commit = []`.
- **Connect** (`base.py:261`): `connect()` sets `run_on_commit = []`.
- I did not read `Atomic.__exit__` in full. `transaction.py` sets `needs_rollback` (lines 271, 293, 304) and calls `connection.commit()` at 276, so the rollback path there appears to flow through the methods above.

**Running**
- **Flag set on commit**: `commit()` (`base.py:323-330`) calls `_commit()` and then sets `self.run_commit_hooks_on_set_autocommit_on = True` (line 330).
- **Hooks run on autocommit**: `set_autocommit(True)` (`base.py:490-492`) checks that flag and calls `self.run_and_clear_commit_hooks()`, then resets the flag. The outermost `Atomic.__exit__` calls `connection.set_autocommit(True)` (`transaction.py:320`), so hooks run after the real commit.
- **`run_and_clear_commit_hooks`** (`base.py:749-766`):
  - It first calls `validate_no_atomic_block()`.
  - It swaps `self.run_on_commit` for a fresh `[]` (751-752), so callbacks registered by a running callback are queued for the next round, not run in this loop.
  - It then pops entries in FIFO order (`pop(0)`) and calls each one.
  - With `robust=True`, exceptions are logged (755-764). Otherwise an exception propagates, and the callbacks still in the local list are lost.

**Uncertainty:** I didn't read the body of `Atomic.__enter__` and `__exit__` in full (`transaction.py` ~200-330). The claim that the outermost exit reaches `set_autocommit(True)` rests on the matches at lines 276 and 320, not on a full read.