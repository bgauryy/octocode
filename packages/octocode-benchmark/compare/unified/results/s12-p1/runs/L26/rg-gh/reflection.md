1. **Helped:** The first Bash call (`grep -n "def _save_table" -A95 django/db/models/base.py`) found the core function and its line numbers in one step. The second call (`sed -n 1165,1215p` plus `grep -n "def _do_update" -A45`) covered the rest of the INSERT path and `_do_update`. Two calls were enough because grep with `-A` returns numbered context.

2. **Did not help:** `git rev-parse HEAD` failed with "could not open /dev/null: Operation not permitted". Every Bash call also printed that same `/dev/null` error. So I could not confirm the checkout was at 4fab678a07, and I said so in the answer. I did not try `gh api` to check the SHA, so that check was never done. I never opened `save()`, `save_base()` or `_save_parents()`, so the per-table inheritance claim is unverified.

3. **Next time:** I would read `.git/HEAD` directly, or call `gh api -X GET repos/django/django/commits/4fab678a07`, to confirm the pin. I would also add a `grep -n "_save_table(" base.py` call to see the callers.

4. **Confidence:** High for the UPDATE/INSERT logic and the cited line numbers, since I read them directly. Medium on the commit pin, because I couldn't verify it.