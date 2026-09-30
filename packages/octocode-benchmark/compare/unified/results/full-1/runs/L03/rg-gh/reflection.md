1. **Helped:** The first Bash call, `git rev-parse HEAD` plus `rg "run_on_commit|on_commit|run_and_clear_commit_hooks" django/db`, confirmed the pinned commit. It also mapped every storage, discard and run site in one pass. The `rg` on `run_commit_hooks_on_set_autocommit_on|run_on_commit` then showed the commit → flag → `set_autocommit` chain. Reading `Atomic.__exit__` in `transaction.py` showed the outermost-block commit path.

2. **Did not help:**
   - My `sed` ranges were sloppy, so the output mixed unrelated code, such as the `set_autocommit` body. Because of that I had to cite lines from the `rg` output instead of from the code I had read.
   - I never viewed the `set_autocommit(True)` call in `Atomic.__exit__`'s `finally` block. I never viewed the full rollback method around `base.py:341` either. Both went into the answer as inferred or flagged gaps.
   - The `cd` calls kept shifting the working directory. That was noise, not an error.

3. **Next time:** I would use `rg -n -C4` on the specific hits instead of guessing `sed` ranges. I would also read the `finally` block of `Atomic.__exit__` to the end, so the full chain is verified. And I would use absolute paths without `cd`.

4. **Confidence:** medium-high. The storage, savepoint-discard and run mechanics were read directly at the pinned commit. The gaps I flagged are the inferred `set_autocommit(True)` step and the exact line of the rollback reset.