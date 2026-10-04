The redirect is done by `CommonMiddleware` in `django/middleware/common.py`, controlled by the `APPEND_SLASH` setting. That setting defaults to `True` (`django/conf/global_settings.py:242`).

**How it works**
1. The request goes through the URL resolver as normal. If the slash-less path matches no view, the response is a 404.
2. `CommonMiddleware.process_response` (`common.py:101-111`) checks for a 404 where `should_redirect_with_slash(request)` is true. If so, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`. The class is the middleware's redirect class, which I did not read; I expect it to be the permanent-redirect (301) class.
3. `should_redirect_with_slash` (`common.py:63-75`) returns True only when all of these hold:
   - `settings.APPEND_SLASH` is on.
   - `request.path_info` does not end with `/`.
   - `is_valid_path(path_info, urlconf)` is false. The `urlconf` comes from `request.urlconf`.
   - `is_valid_path(path_info + "/", urlconf)` does match.
   - The matched view's `should_append_slash` attribute is true. It defaults to `True` via `getattr`, so a view can opt out. I did not look for the decorator that sets it.
4. `get_full_path_with_slash` (`common.py:77-99`) builds the target URL:
   - It calls `request.get_full_path(force_append_slash=True)`. That is defined in `django/http/request.py:220-237` and adds `/` to the path when it is missing. It also keeps the query string.
   - It runs `escape_leading_slashes` on the result, so the redirect can't become a scheme-relative URL.
   - If `DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` instead. A redirect can't keep the request body, so Django warns the developer.

**PREPEND_WWW interaction**
`process_request` (`common.py:51-61`) also uses these helpers. When `PREPEND_WWW` applies and a slash is needed, it does both changes in a single redirect to `www.<host><path-with-slash>`. That check is made up front, not only after a 404.

**Uncertainty**
I read only `common.py` and `request.py`. I did not read `is_valid_path` or the redirect class definition.