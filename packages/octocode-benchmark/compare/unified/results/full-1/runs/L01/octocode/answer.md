With `CommonMiddleware`, a 404 on a path with no trailing slash becomes a permanent (301) redirect to the slash version, but only if the slash version matches a URL pattern. The code is in `django/middleware/common.py`. I read that file at the pinned commit. The line numbers below are counted from the returned text, so treat them as approximate (about ±2). I did not open `global_settings.py` or the URL resolver.

**How it works**

1. **Setting and class.** The behavior is controlled by `settings.APPEND_SLASH`. `CommonMiddleware` (class starts ~line 13) has `response_redirect_class = HttpResponsePermanentRedirect` (~line 34), so the redirect is a 301. Subclasses can override that attribute.

2. **Check in `process_response`.** The view runs first with the original path. In `process_response` (~line 101–115), if `response.status_code == 404 and self.should_redirect_with_slash(request)`, the response is replaced with `self.response_redirect_class(self.get_full_path_with_slash(request))`.

3. **Decision in `should_redirect_with_slash`** (~lines 64–76). It returns True only if all of these hold:
   - `settings.APPEND_SLASH` is on.
   - `request.path_info` doesn't end with `/`.
   - `is_valid_path(request.path_info, urlconf)` is false. This function is imported from `django.urls`.
   - `is_valid_path(path_info + "/", urlconf)` matches. The `urlconf` is `request.urlconf` if set.
   - The matched view's `should_append_slash` attribute is true. It defaults to True and is read with `getattr(view, "should_append_slash", True)`. A view can therefore opt out, for example via a decorator that sets it to False.

4. **Building the target in `get_full_path_with_slash`** (~lines 78–99):
   - It calls `request.get_full_path(force_append_slash=True)`, which keeps the query string.
   - It runs `escape_leading_slashes()` on the result to avoid producing a scheme-relative URL such as `//evil.com`.
   - If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError`. A redirect would lose the request body, so the error tells the developer to fix the form action or set `APPEND_SLASH=False`.

5. **`PREPEND_WWW` case** (in `process_request`, ~lines 52–62). If `PREPEND_WWW` is on and the host lacks `www.`, the middleware redirects in `process_request`. It calls `should_redirect_with_slash` there too, so the `www.` and the trailing slash are added in a single redirect. This is a second entry point that redirects before the view runs.

**Uncertainty:** I did not verify the internals of `is_valid_path`, the `should_append_slash` decorator, or the `APPEND_SLASH` default in `global_settings.py`.