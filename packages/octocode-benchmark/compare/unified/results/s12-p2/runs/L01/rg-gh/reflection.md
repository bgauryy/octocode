1. **Helped:** The second Bash call, a `grep -nE` for `APPEND_SLASH|should_redirect_with_slash|get_full_path...` across `django/middleware/common.py`, `django/http/request.py` and `django/urls/base.py`, gave exact line anchors in one shot. The third call, `sed -n 35,125p common.py` plus `sed -n 215,245p request.py`, then showed the whole mechanism. Two calls were enough because I already knew where `CommonMiddleware` lives.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, even though the instructions said it was. That call was wasted.
   - Every Bash call printed `/dev/null: Operation not permitted`. This was harmless but noisy.
   - I never confirmed that the checkout was actually at 4fab678a07, because I never ran `git rev-parse HEAD`. I relied on the task statement.
   - I never opened `response_redirect_class` or the `no_append_slash` decorator. The answer says so, but the line-72 claim about the opt-out is partly inferred.
   - I never looked at the tests or the docs.

3. **Next time:** I'd use `grep` from the start. I'd add `grep -n "response_redirect_class\|no_append_slash" -r django/` and a `git rev-parse HEAD` check to the same call. That would close the gaps.

4. **Confidence:** High on the core flow, since I read the code directly. Medium on the two unverified details above.