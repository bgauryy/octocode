1. **Helped:** The first Bash call combined `git rev-parse HEAD` with one `rg` for `APPEND_SLASH|should_redirect_with_slash|get_full_path...` over `django/middleware/common.py` and `django/http/request.py`. It confirmed the pinned commit and located every relevant function in one step. I guessed those two file paths from prior knowledge of Django. The second call printed `common.py:33-124`, `request.py:215-250`, and the `APPEND_SLASH` default in `global_settings.py`, which was enough to answer.

2. **Did not help:** Nothing failed and I made no repeated calls. Two gaps remain:
   - I never opened `is_valid_path`, the `no_append_slash` decorator, or the tests. The answer says so, and those statements are unverified.
   - I did not run the behavior.
   - The working directory changed between calls. I had `cd`'d into the repo, so the change was harmless, but it shows paths shouldn't be assumed.

3. **Next time:** I would add one `rg` for `should_append_slash|def is_valid_path` across `django/`, so the opt-out mechanism is verified rather than described from memory. I would also check `git log` or the tests for context if the question called for history.

4. **Confidence:** High for the flow and the line citations, since I read them directly at the verified commit. Medium for the opt-out detail, because I didn't read the decorator.