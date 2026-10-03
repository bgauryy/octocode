The redirect is done by `CommonMiddleware` in `django/middleware/common.py`. It runs after the view, only when the response is a 404. It sends a 301 redirect to the slash URL if adding a slash would match a URL pattern.

**How it works**
1. **Setting.** `APPEND_SLASH` defaults to `True` (`django/conf/global_settings.py:242`).
2. **Redirect class.** `response_redirect_class = HttpResponsePermanentRedirect` (`common.py:33`), so the redirect is a 301.
3. **Trigger.** `process_response` (`common.py:101-112`) checks for `response.status_code == 404 and self.should_redirect_with_slash(request)`. If both hold, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`. The redirect therefore happens only after the normal URL resolution and view have returned a 404.
4. **Decision.** `should_redirect_with_slash` (`common.py:63-75`) returns True only if all of these hold:
   - `APPEND_SLASH` is on.
   - `request.path_info` doesn't already end in `/`.
   - `is_valid_path(path_info, urlconf)` is false, meaning the path as given doesn't resolve.
   - `is_valid_path(path_info + "/", urlconf)` matches a pattern.
   - The matched view's `should_append_slash` attribute is true. It defaults to `True` when the attribute is absent. The `no_append_slash` decorator is the usual way to opt out, but I didn't open it.
5. **Building the URL.** `get_full_path_with_slash` (`common.py:77-99`) calls `request.get_full_path(force_append_slash=True)`. That goes through `HttpRequest._get_full_path` (`django/http/request.py:220-236`), which adds `/` if the path lacks one and keeps the query string. The result then goes through `escape_leading_slashes` so it can't become a scheme-relative URL.
6. **DEBUG guard.** If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` instead of redirecting (`common.py:87-99`). A redirect would lose the request body.

**PREPEND_WWW case.** `process_request` (`common.py:35-61`) handles this earlier. If `PREPEND_WWW` is on and the host lacks `www.`, it calls `should_redirect_with_slash` and appends the slash in the same redirect to `www.<host>`. That way the client follows one redirect instead of two.

I read the code but didn't run it, and I didn't open the `no_append_slash` decorator or `is_valid_path`.