The redirect is done by `CommonMiddleware` in `django/middleware/common.py`, controlled by the `APPEND_SLASH` setting. That setting defaults to `True` (`django/conf/global_settings.py:242`).

**Flow**
1. The URL is resolved normally first. If it doesn't match any URL pattern, the response is a 404.
2. `CommonMiddleware.process_response` (`common.py:101-111`) checks for that 404. If `response.status_code == 404` and `should_redirect_with_slash(request)` is true, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`. The redirect class is the middleware's `response_redirect_class`. I didn't read where that is defined, so I can't say which status code it produces.
3. `should_redirect_with_slash` (`common.py:63-75`) returns true only when all of these hold:
   - `settings.APPEND_SLASH` is true and `request.path_info` doesn't end in `/`.
   - `request.path_info` is not a valid path, checked with `is_valid_path(path_info, urlconf)`. The urlconf comes from `request.urlconf` if one is set.
   - `path_info + "/"` is a valid path.
   - The matched view's `should_append_slash` attribute is true. It defaults to `True` (`common.py:73-74`), so a view can opt out of the redirect. I didn't look up the decorator that sets it.
4. `get_full_path_with_slash` (`common.py:77-99`) builds the target:
   - It calls `request.get_full_path(force_append_slash=True)`, which keeps the query string. The method is defined at `django/http/request.py:220`; I didn't read `_get_full_path`.
   - It runs the result through `escape_leading_slashes` so the redirect can't become a scheme-relative URL.
   - If `DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` instead (`common.py:87-98`). The reason given is that a redirect would lose the request body.

**Related behavior**
- With `PREPEND_WWW` on, `process_request` (`common.py:51-61`) uses the same check to add the slash and the `www.` prefix in a single redirect.
- A `Referer` check at `common.py:180-183` treats a referer equal to the URL without its slash as an internal redirect. This is in code I only saw as a search snippet, so I'm not sure which method contains it.