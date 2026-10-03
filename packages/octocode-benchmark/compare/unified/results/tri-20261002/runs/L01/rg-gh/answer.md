The redirect is done by `CommonMiddleware` in `django/middleware/common.py`. It is controlled by the `APPEND_SLASH` setting, which defaults to True. I read the code but did not run it, and I did not check the `APPEND_SLASH` default or the `no_append_slash` decorator.

**How it works**
1. The request goes through the URL resolver as normal. If nothing matches, the response is a 404.
2. `CommonMiddleware.process_response` (`common.py:101-112`) checks for that 404. If the status is 404 and `should_redirect_with_slash(request)` returns True, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`.
3. `response_redirect_class` is `HttpResponsePermanentRedirect` (`common.py:33`), so the redirect is a 301.
4. `should_redirect_with_slash` (`common.py:63-75`) returns True only when all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` does not end with `/`.
   - The original path does not resolve. This uses `is_valid_path`, which wraps `resolve()` and returns False on `Resolver404` (`django/urls/base.py:169-178`).
   - The path with `/` appended does resolve.
   - The matched view's `should_append_slash` attribute is not False. It defaults to True, so a view can opt out.
5. `get_full_path_with_slash` (`common.py:77-99`) builds the new URL with `request.get_full_path(force_append_slash=True)`.
   - That call appends the `/` and keeps the query string (`django/http/request.py:220-235`).
   - The path then goes through `escape_leading_slashes`, which stops it becoming a scheme-relative URL.

**Other behaviour**
- **POST/PUT/PATCH/DELETE in DEBUG:** with `settings.DEBUG` on, these methods raise a `RuntimeError` instead of redirecting (`common.py:87-98`). A redirect would lose the request body.
- **`PREPEND_WWW`:** `process_request` (`common.py:48-58`) also applies the slash check there. This lets one redirect handle both the `www.` prefix and the slash.