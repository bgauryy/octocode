**1. Helped:** The first `localSearch` call was the fastest step. I ran one regex (`APPEND_SLASH|def should_redirect_with_slash|def process_response…`) on `django/middleware/common.py`. It found the class, the decision method and the 404 hook in one call. The `localFetch` of lines 63-120 then gave the whole mechanism with exact line numbers, so I could cite it directly.

**2. Did not help:**
- My first `localFetch` failed validation. I wrote the ranges with curly quotes, and the error message said to use `"start-end"`. That cost one call and was my formatting mistake.
- The `localSearch` output was noisy. Its context snippets overlapped, and it matched `process_response` in an unrelated middleware (`BrokenLinkEmailsMiddleware`).
- I never searched for or opened `is_valid_path`, `HttpRequest.get_full_path` or `escape_leading_slashes`. No tool failed there. I just didn't look, so the answer says those parts are inferred.

**3. Next time:** I would run `localSearch` for `def is_valid_path` and `def get_full_path` in the same first batch. I would also add one lookup for how `should_append_slash` gets set, probably the `no_append_slash` decorator. I would write the `localFetch` JSON carefully the first time.

**4. Confidence:** High for the middleware flow, because I read those lines directly. Medium for what the three unread helpers do, because I only inferred that from their names and call sites.