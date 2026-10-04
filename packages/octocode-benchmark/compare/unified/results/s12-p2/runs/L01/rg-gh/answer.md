**Short answer:** `CommonMiddleware` does it, in `django/middleware/common.py`. It's controlled by the `APPEND_SLASH` setting. The redirect happens only after the URL resolver has failed (a 404 response). Django then checks whether the same path plus `/` would match a URL pattern. If it would, the middleware replaces the 404 with a redirect to the slashed URL.

**Flow**
1. `CommonMiddleware.process_response` (`common.py:101-115`) checks `response.status_code == 404 and self.should_redirect_with_slash(request)` (`:108`). If both hold, it replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`. The redirect class is the permanent one, per the class attribute. I didn't view that line.
2. `should_redirect_with_slash` (`common.py:63-75`) returns True only if all of these hold:
   - `settings.APPEND_SLASH` is on.
   - `request.path_info` doesn't end in `/`.
   - The original path is not valid: `not is_valid_path(request.path_info, urlconf)`.
   - The path with `/` appended is valid: `is_valid_path("%s/" % request.path_info, urlconf)` returns a match. `is_valid_path` is defined in `django/urls/base.py:169`.
   - The matched view's `should_append_slash` attribute is truthy (default True, `:72`). A view can opt out with the `no_append_slash` decorator. I didn't open that decorator, so I'm inferring that it sets this attribute.
3. `get_full_path_with_slash` (`common.py:77-99`) builds the target URL:
   - It calls `request.get_full_path(force_append_slash=True)`. `_get_full_path` in `django/http/request.py:220-236` adds the `/` and keeps the query string.
   - It runs `escape_leading_slashes` to avoid producing a scheme-relative URL.
   - If `DEBUG` is on and the method is POST, PUT, PATCH or DELETE, it raises a `RuntimeError`. A redirect would lose the request body, so Django tells the developer to fix the URL instead.

**PREPEND_WWW interaction:** in `process_request` (`common.py:35-61`), if `PREPEND_WWW` applies and a slash is also needed, the slash check runs up front (`:56-57`). That way a single redirect to `www.host/path/` covers both changes.

**Related:** `BrokenLinkEmailsMiddleware` (`common.py:~180-182`) ignores 404s where the referer equals the current URL minus the trailing slash. This avoids reporting the redirect-triggering case as a broken link.

**Uncertainty:** I read only `common.py` and `request.py`. I didn't confirm the `response_redirect_class` default or the `no_append_slash` decorator.