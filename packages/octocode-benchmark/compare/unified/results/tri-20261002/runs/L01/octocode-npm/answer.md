The redirect is done by `CommonMiddleware` in `django/middleware/common.py`, controlled by the `APPEND_SLASH` setting. It defaults to `True` (`django/conf/global_settings.py:242`). I read the middleware code, but I did not read `is_valid_path`, `get_full_path` or `no_append_slash` in full, and I did not run anything.

**Flow**

1. **The 404 check.** `process_response` (`common.py:101-111`) looks at the response. If the status is 404 and `should_redirect_with_slash(request)` is true, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`.
   - The redirect class is the middleware's `response_redirect_class` attribute, which I did not read. I'm assuming it is the permanent redirect (301), but I haven't confirmed that.
   - The URL is only tried in the normal way first. The slash redirect happens only after the view lookup has already returned a 404.

2. **The decision.** `should_redirect_with_slash` (`common.py:63-75`) returns `True` only when all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` doesn't end with `/`.
   - `is_valid_path(request.path_info, urlconf)` is false, so the URL as requested doesn't resolve.
   - `is_valid_path("<path>/", urlconf)` does resolve. It uses `request.urlconf` if one is set.
   - The matched view's `should_append_slash` attribute is true (default `True`). Views marked with `@no_append_slash` (`django/views/decorators/common.py:7`) opt out.

3. **Building the target.** `get_full_path_with_slash` (`common.py:77-99`) does three things:
   - It calls `request.get_full_path(force_append_slash=True)`, which keeps the query string.
   - It runs the result through `escape_leading_slashes`, so the redirect can't become a scheme-relative URL.
   - If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` instead. A redirect would lose the request body, so the error tells the developer to fix the form's URL or set `APPEND_SLASH=False`.

4. **The `PREPEND_WWW` case.** `process_request` (`common.py:51-61`) also calls `should_redirect_with_slash` when `PREPEND_WWW` is on and the host lacks `www.`. This lets one redirect add both the `www.` prefix and the slash.

**Related code elsewhere**
- `django/middleware/locale.py:59` has a similar check in `LocaleMiddleware`, for paths with a language prefix.
- `django/contrib/admin/sites.py:470` does its own check in `catch_all_view`, which is decorated with `@no_append_slash`.
- `django/urls/resolvers.py:173` skips the "pattern starts with a slash" check when `APPEND_SLASH` is off.