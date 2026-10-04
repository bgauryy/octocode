1. **Helped:** The second call, `grep -rn "run_on_commit|on_commit|run_commit_hooks_on_set_autocommit_on" django/db`, found every relevant site at once. The batched `sed -n` ranges in the next call then gave the code for storage, discard and run in a single round trip.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so it was wasted.
   - Every Bash call printed `/dev/null: Operation not permitted`. This was harmless noise, but it could hide real errors.
   - My `transaction.py` sed ranges (268-282, 296-345) were guesses. They cut off the `Atomic.__exit__` `finally` block, so I cited it as `~318-322`. That is an approximate line, not a verified one.
   - I never opened `Atomic.__enter__`, so I couldn't say how `savepoint_ids` is populated, and I flagged that in the answer.
   - I never ran code. The claim that a callback registered inside a callback runs immediately is inferred from the source, not tested.

3. **Next time:** Use `grep -n` from the start. Pull the whole `Atomic` class with `sed -n 175,330p`, or grep for `def __enter__` first. Then confirm the exact lines and check the tests in `tests/transaction_hooks`.

4. **Confidence:** High for the core mechanism, because I read the `base.py` lines directly. Medium for the `transaction.py` line numbers and the nested-callback behaviour.