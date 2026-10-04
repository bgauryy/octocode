**Short answer:** `CommonMiddleware` does it. If a request gets a 404, the path has no trailing slash, and the same path with a slash matches a URL pattern, the middleware swaps the 404 for a permanent redirect (301) to the slash URL. It only runs when `settings.APPEND_SLASH` is true. Django's docs describe `APPEND_SLASH` as defaulting to True, but I didn't check the default in this checkout.

All paths below are in `django/middleware/common.py` at 4fab678a07.

**Flow**
1. **Trigger (`process_response`, lines 101–112).**
   - The request goes through normal URL resolution first.
   - If the response status is 404 and `should_redirect_with_slash(request)` is true, the response is replaced (lines 108–111).
   - The replacement is `self.response_redirect_class(self.get_full_path_with_slash(request))`.
   - `response_redirect_class` is `HttpResponsePermanentRedirect` (line 33), so the redirect is a 301.

2. **Decision (`should_redirect_with_slash`, lines 63–75).** It returns True only if all of these hold:
   - `settings.APPEND_SLASH` is true and `request.path_info` doesn't end with `/` (line 68).
   - The path as given is not valid: `is_valid_path(request.path_info, urlconf)` fails (line 70).
   - The path with `/` appended is valid: `is_valid_path("%s/" % request.path_info, urlconf)` matches (line 71).
   - The matched view's `should_append_slash` attribute is true. It is read with `getattr(view, "should_append_slash", True)` (lines 72–73), so a view can opt out of the redirect. I didn't look up which decorator sets that attribute.

3. **Building the target (`get_full_path_with_slash`, lines 77–99).**
   - It calls `request.get_full_path(force_append_slash=True)` (line 83). That helper, `_get_full_path` in `django/http/request.py:226-235`, adds `/` if the path lacks one and keeps the query string.
   - It then runs `escape_leading_slashes` to avoid building a scheme-relative URL (line 85).
   - If `DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` (lines 86–98). The message says Django can't keep the request body through a redirect.

4. **Related behaviour in `process_request` (lines 35–61).** When `PREPEND_WWW` also applies, the middleware calls `should_redirect_with_slash` there too (line 56). That way one redirect adds both the `www.` prefix and the slash.

**Uncertainty:** I only read `common.py` and the `get_full_path` helpers in `request.py`. I didn't open `is_valid_path` or the `should_append_slash` definition.