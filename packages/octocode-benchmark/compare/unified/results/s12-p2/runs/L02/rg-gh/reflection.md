**1. Helped:**
- The second Bash call was `grep -n "unsaved\|prohibited" django/db/models/base.py`. It found the error message at lines 1301 and 1328 right away.
- The third call combined `sed -n 1275,1345p base.py` with `grep -rn "_prepare_related_fields_for_save" django`. It showed the whole function and all of its callers in one step.

**2. Did not help:**
- The first call failed. `rg` isn't installed, even though the prompt lists it, and `git rev-parse HEAD` failed with "/dev/null: Operation not permitted". Every later call also printed that `/dev/null` noise.
- I never confirmed the checkout is at 4fab678a07.
- `sed` printed no line numbers. Apart from the grep-confirmed 1301 and 1328-1329, the line numbers I cited (1295, 1297-1298, 1303-1307, 1309-1313, 1315-1330) were counted by hand from line 1276. I stated them as exact without saying so.
- I never read `base.py:864` or `query.py:1042`.

**3. Next time:** use `grep -n` or `sed -n` with `cat -n` so line numbers are printed. Confirm the SHA by reading `.git/HEAD`, since `git` fails here. Look at the two callers I skipped.

**4. Confidence:** high on the mechanism and the function's location, because I read the code and grep confirmed the location. Medium on most of the sub-line citations, because I counted them by hand and may be off by one or two.