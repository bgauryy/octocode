Django does this in `CommonMiddleware`, in `django/middleware/common.py`. It issues a permanent redirect when the path has no trailing slash, no URL pattern matches it, and a pattern does match it with a slash added. The `APPEND_SLASH` setting (default `True`) turns the behavior on. I read the file but did not check the setting's default in `global_settings.py`, so that default comes from memory. Line numbers below are counted by hand from the file content, so they are accurate for the early lines and approximate for the later ones.

**How it works**
1. **Redirect class.** `response_redirect_class = HttpResponsePermanentRedirect` (line 33), so the redirect is a 301. A subclass can override it.
2. **Decision logic.** `should_redirect_with_slash(request)` (about line 63) returns `True` only when all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` does not end with `/` (line 68).
   - `is_valid_path(request.path_info, urlconf)` is false, meaning the URL resolver has no match for the original path (line 70).
   - `is_valid_path("%s/" % request.path_info, urlconf)` matches (line 71).
   - The matched view's `should_append_slash` attribute is true, defaulting to `True` (lines 72–74). The `no_append_slash` decorator uses this to opt a view out.
3. **Building the target.** `get_full_path_with_slash(request)` (about line 77) calls `request.get_full_path(force_append_slash=True)`, which keeps the query string. It then calls `escape_leading_slashes` so the result can't become a scheme-relative URL (lines 84–86).
4. **Method check in debug mode.** With `DEBUG=True` and a `DELETE`, `POST`, `PUT` or `PATCH` request, it raises a `RuntimeError` instead. A redirect would lose the request body, so this warns the developer to fix the form URL or turn off `APPEND_SLASH` (about lines 87–99).
5. **When it runs.**
   - **`process_response` (about line 102, the normal path).** If the response status is 404 and `should_redirect_with_slash` is true, the 404 response is replaced with the redirect. A URL that resolves normally is never redirected.
   - **`process_request` (lines 51–61).** With `PREPEND_WWW` on and a host that doesn't start with `www.`, the same check runs up front. That way one redirect both adds `www.` and appends the slash.

The class docstring (lines 13–30) describes the same behavior. `is_valid_path` is imported from `django.urls` (line 9). I did not open its implementation.