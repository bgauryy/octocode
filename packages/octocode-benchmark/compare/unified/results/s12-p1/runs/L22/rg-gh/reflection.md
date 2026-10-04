1. **Helped:** The second call, a `grep -rnE` over `django` for `def static\b|\bstatic\(|import.*\bstatic\b|static_tag|templatetags.static`, found all three `static` definitions and every call site in one pass. The third call only printed the function body at `django/templatetags/static.py:174-180`, which confirmed the docstring.

2. **Did not help:** `rg` isn't installed, so the first call was wasted. Every shell call also printed `/bin/bash: /dev/null: Operation not permitted`. In the third call that noise made an empty result ambiguous, because I couldn't tell "no matches" from "grep failed." I also never opened the call sites, so my line numbers come only from grep output. I didn't check whether `static.static` or multi-line imports exist.

3. **Next time:** Check tool availability first. Use `grep -rn ... 2>&1` so errors are visible. Add a `grep -rn "static\.static\|import (.*static"` pass for other import styles. Open `widgets.py` and `admin_list.py` at the cited lines to confirm.

4. **Confidence:** Medium-high. The two importers and their call lines come straight from grep. The remaining risk is a missed caller, since the negative-result search was ambiguous.