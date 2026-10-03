1. **Helped:** I guessed the right file, `django/middleware/common.py`, from prior knowledge. One `localFetch` of lines 1–130 returned the whole `CommonMiddleware` logic, including `should_redirect_with_slash`, `get_full_path_with_slash` and `process_response`. That one call answered the question.

2. **Did not help:**
   - My first `localSearch` failed with a validation error. I passed `include` as a JSON string instead of a proper array. It cost one call, but no information.
   - `localFetch` returned a path prefixed `python/django/...`, which didn't match the checkout path I'd given. I didn't investigate it.
   - I never read `is_valid_path` in `django/urls`, and I never read `global_settings.py` for the `APPEND_SLASH` default. The default therefore rests on memory, and I said so.
   - I never called `lspSearch`, and I didn't check the `no_append_slash` decorator. My claim that it sets `should_append_slash` comes from memory.
   - Line numbers after about line 60 are my hand count from the content, not tool-provided. I flagged them as approximate.

3. **Next time:** I'd batch the `localFetch` of `common.py` with a `localSearch` for `should_append_slash` and `APPEND_SLASH`, using a correctly formed `include` array. I'd use `matchString` reads, which return exact line numbers, instead of counting lines by hand.

4. **Confidence:** High on the mechanism, because I read it directly. Medium on the exact line numbers after line 60 and on the unverified `APPEND_SLASH` default and `no_append_slash` claims.