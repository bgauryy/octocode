**1. Helped:**
- The first `localSearch` was the key call. One regex (`run_on_commit|def on_commit|savepoint_ids|run_and_clear_commit_hooks`) over `django/db` returned nearly every relevant site, with context lines. That covered storage, the savepoint-rollback filter, the resets, and the hook runner.
- The second `localSearch` for `run_commit_hooks_on_set_autocommit_on` showed how the flag links `commit()` to `set_autocommit`.
- The `localFetch` ranges on `base.py` (320-342) and `transaction.py` (240-335) confirmed the commit and rollback flow in `Atomic.__exit__`.

**2. Did not help:**
- The first `localFetch` was wasteful. I requested three ranges, and it printed "lines omitted" gaps, so part of its output repeated what the search had already shown (`base.py:727-766`).
- Fetching `transaction.py:142-152` was redundant, since the search had already shown `on_commit`.
- The `localSearch` warning about 1226 skipped `.mo` files was noise.
- I never fetched the `savepoint_rollback` function header. I only saw its body through search context, so I did not confirm the surrounding method.

**3. Next time:**
- Skip fetches for lines the search already returned.
- Use fewer, tighter ranges.
- Run one `lspSearch` for references to `run_on_commit` to confirm there are no other writers.

**4. Confidence:** High. Every claim cites lines I saw at the pinned checkout. The remaining gap is the unread `savepoint_commit` and `savepoint_rollback` bodies, which I flagged in the answer.