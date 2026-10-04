**Helped:** The second Bash call did the work. It ran `grep -n "self\._chain("` on `django/db/models/query.py` and, in the same call, listed every `def`/`class` line with `grep -n`. Matching call lines against the `def` list gave the method names without opening any bodies.

**Did not help:** The first Bash call failed. `rg` isn't installed, and `git rev-parse HEAD` died on `/dev/null: Operation not permitted`. So I never confirmed the checkout was at 4fab678a07, and I said so. `/dev/null` errors also cluttered every call.

**Error in my answer:** I wrote "thirty-one lines in 26 methods". The grep output shows 24 call lines in 23 methods, which matches my table. The summary count was wrong and I never recounted. I never opened any method body to check my line-to-method mapping.

**Next time:** Start with `grep`, since `rg` is missing. Count the output with `grep -c` before stating totals. Spot-check a few call sites with `sed -n`. Pin the commit with `gh api` if `git` is blocked.

**Confidence:** Medium-high on the method list, because it comes straight from grep output and every line falls inside a plausible `def` range. Low on the stated totals, which are wrong. The commit pin is unverified.