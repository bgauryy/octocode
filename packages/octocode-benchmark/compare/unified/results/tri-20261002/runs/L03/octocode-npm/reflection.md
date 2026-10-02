**1. Helped:**
- The first parallel `localSearch` (`run_on_commit` plus `def on_commit`) located the whole storage/discard surface in `base.py` and `transaction.py` in one round trip.
- The batched `localGetFileContent` with exact line ranges gave citable bytes for `on_commit`, `run_and_clear_commit_hooks` and `savepoint_rollback`.
- The `localSearch` for `run_and_clear_commit_hooks` with `contextLines: 6` and `exclude: tests/**` found the only non-test caller. The follow-up search on `run_commit_hooks_on_set_autocommit_on` found the flag that triggers it.

**2. Did not help:**
- I made a stray `localSearch` on `transaction.py` for `savepoint_ids` and similar terms that added little. I also had to guess that the caller lived in `set_autocommit`.
- Match snippets were truncated, so I needed a further read to see the surrounding code.
- I never read `rollback()`'s `def` line. I inferred that line 341 belongs to it from the neighbouring lines.
- I used no LSP references, which would have proven the caller set directly.

**3. Next time:** I'd run `lspGetSemantics` references on `run_and_clear_commit_hooks` and read `base.py:320-345` and `483-495` directly.

**4. Confidence:** high for storage, discard and run order. One claim in my answer is wrong. I said a nested `on_commit` registered during hook execution "appends to the new list" and also "runs immediately". Since `in_atomic_block` is False at that point and autocommit is on, it runs immediately and is not appended. I didn't verify that path by reading the code or running it.