1. **Helped:** The first Bash call, `grep -n "def _save_table" -A 95 django/db/models/base.py`, found the function and showed most of the UPDATE/INSERT logic in one shot. The second call, `sed -n 1165,1215p` plus `grep -n "def _do_update" -A 40`, covered the INSERT fallback and `_do_update` together. Two calls were enough because the target was one well-named function.

2. **Did not help:** Every Bash call printed `/dev/null: Operation not permitted`. That is sandbox noise, but it did cause a real failure: `git rev-parse HEAD` died with `fatal: could not open '/dev/null'`. So I never confirmed the checkout was at 4fab678a07. I said so in the answer, but I couldn't check it.

3. **Next time:** Read `.git/HEAD` or the packed refs directly with `cat`, since `git` is unusable here. I would also read the whole function once, then trace `_do_insert` and `save_base` to confirm callers. I didn't do that, and it wasn't needed for this question.

4. **Confidence:** High. Line numbers come directly from the printed output. The only gap is that the commit pin is unverified, though the checkout was described as pinned.