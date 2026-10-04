The redirect is done by `CommonMiddleware` in `django/middleware/common.py`, controlled by the `APPEND_SLASH` setting. It runs as a 404 fallback on the response, not as a rewrite of the request before URL resolution.

1. **Trigger (`process_response`, lines 101–111).** The middleware lets the request go through normal URL resolution first. If the response status is 404 and `should_redirect_with_slash(request)` returns True, it swaps the response for `self.response_redirect_class(self.get_full_path_with_slash(request))`. `response_redirect_class` is `HttpResponsePermanentRedirect` (line 33), so the redirect is a 301.

2. **Decision (`should_redirect_with_slash`, lines 63–75).** It returns True only when all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` doesn't end with `/` (line 68).
   - The path as given is not a valid URL: `not is_valid_path(request.path_info, urlconf)` (line 70). `urlconf` comes from `request.urlconf` if it is set.
   - The path with `/` appended does resolve (line 71).
   - The matched view's `should_append_slash` attribute is not False. It defaults to True (lines 73–74). This is what lets a view opt out of the redirect.

3. **Building the URL (`get_full_path_with_slash`, lines 77–99).** It calls `request.get_full_path(force_append_slash=True)`, which keeps the query string. It then runs `escape_leading_slashes` so the result can't become a scheme-relative URL (lines 84–86).

4. **DEBUG guard (lines 87–98).** If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError`. A redirect would lose the request body, so the error tells the developer to fix the form's action URL or set `APPEND_SLASH=False`.

Two related details:
- Line 182 shows the broken-link email middleware also knows about this. When `APPEND_SLASH` is on and the referer equals the URL without its trailing slash, it treats that as an internal redirect.
- I only read `common.py`. I didn't open `is_valid_path`, `get_full_path` or `escape_leading_slashes`, so how they work internally comes from their names and call sites.