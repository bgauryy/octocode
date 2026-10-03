1. **Helped:** The first Bash call, `rg -n` over `base/base.py` and `transaction.py` for `run_on_commit`, `on_commit`, `savepoint_*` and similar, gave nearly the whole map in one shot. The second call, several `sed -n` ranges chained in one command, supplied the actual code for storage, discard and run.

2. **Did not help:** Every Bash call printed `/dev/null: Operation not permitted`. `git rev-parse HEAD` failed because of it, so I never confirmed the checkout was at the pinned commit. The chained `sed` output had no line numbers. Because of that, I gave some `transaction.py` lines only as approximations (`~310-316`, `~299-303`, `~319-324`). I should have fetched them properly.

3. **Next time:** Use `rg -n` or `cat -n` with the line ranges I need so every cited line is exact. I would also try `git -C <repo> log -1` or read `.git/HEAD` to confirm the commit. I'd check `set_autocommit` more closely and check `django/test/testcases.py` for `captureOnCommitCallbacks` if the question needed it.

4. **Confidence:** High on the mechanism and the `base.py` line numbers, since I read that code directly. Medium on the approximate `transaction.py` lines. The commit pin is unverified.