**Short answer:** `CommonMiddleware` does it, and only when `settings.APPEND_SLASH` is `True`, which is the default (`django/conf/global_settings.py:242`). It lets the request run normally. If the response is a 404 and the same path with a `/` added would resolve, it replaces the 404 with a permanent redirect to the slashed URL. All line numbers below are in `django/middleware/common.py`.

**Flow**
1. `process_response` (line 101) checks `response.status_code == 404 and self.should_redirect_with_slash(request)` (line 108). If both hold, it returns `self.response_redirect_class(self.get_full_path_with_slash(request))`.
2. `response_redirect_class = HttpResponsePermanentRedirect` (line 33), so the redirect is a 301.
3. `should_redirect_with_slash` (lines 63-75) returns True only if all of these hold:
   - `APPEND_SLASH` is on and `request.path_info` doesn't end with `/` (line 68).
   - The path as given does not resolve, checked with `is_valid_path(path_info, urlconf)`.
   - The path with `/` appended does resolve (`match = is_valid_path("%s/" % request.path_info, urlconf)`).
   - The matched view's `should_append_slash` attribute is truthy, and it defaults to `True` (line 74). A view can opt out with that attribute.
4. `get_full_path_with_slash` (lines 77-98) builds the new path:
   - It calls `request.get_full_path(force_append_slash=True)`, which keeps the query string.
   - It passes the result through `escape_leading_slashes`, so the redirect can't become a scheme-relative URL.
   - If `DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError`. A redirect would lose the request body, so Django warns you instead.

**`PREPEND_WWW`:** `process_request` (lines 35-61) handles this case. If `PREPEND_WWW` is on and the host lacks `www.`, it calls `should_redirect_with_slash` and `get_full_path_with_slash` up front. That way a single redirect both adds `www.` and appends the slash (lines 48-59).

**Related:** `django/middleware/locale.py:59` also checks `APPEND_SLASH`. I didn't read it, so I can't say how it uses the setting.

I didn't open `request.get_full_path`, `is_valid_path` or `escape_leading_slashes`. What they do is inferred from how they're called and named.