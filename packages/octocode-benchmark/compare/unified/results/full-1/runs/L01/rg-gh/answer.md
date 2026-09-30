The redirect is done by `CommonMiddleware` in `django/middleware/common.py`. It is controlled by the `APPEND_SLASH` setting, which defaults to `True` (`django/conf/global_settings.py:242`). It is a permanent redirect (301) that only happens after the URL has already 404'd.

**How it works**

1. **Redirect class.** `response_redirect_class = HttpResponsePermanentRedirect` (`common.py:33`).
2. **Normal handling comes first.** The request goes through the URL resolver and view as usual. If the response is anything other than 404, nothing happens.
3. **404 check.** `CommonMiddleware.process_response` (`common.py:101-112`) runs when `response.status_code == 404` and `self.should_redirect_with_slash(request)` is true. It then replaces the response with `self.response_redirect_class(self.get_full_path_with_slash(request))`.
4. **`should_redirect_with_slash`** (`common.py:63-75`) returns true only if all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` doesn't end with `/`.
   - The path as given isn't a valid URL: `not is_valid_path(request.path_info, urlconf)`.
   - The path with `/` added does resolve: `is_valid_path("%s/" % request.path_info, urlconf)` returns a match.
   - The matched view doesn't opt out. `getattr(view, "should_append_slash", True)` must be true. This attribute is what lets a view opt out, for example via a decorator that sets it to `False`. I didn't check where that attribute is set.
5. **`get_full_path_with_slash`** (`common.py:77-99`) builds the target URL:
   - It calls `request.get_full_path(force_append_slash=True)`. That is implemented in `django/http/request.py:220-236` (`_get_full_path`), which adds `/` if the path lacks one and keeps the escaped query string.
   - It runs the result through `escape_leading_slashes` so the redirect can't become a scheme-relative URL.
   - If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError`. A redirect would lose the request body, so this tells the developer to fix the URL instead.

**Combined with `PREPEND_WWW`**

In `process_request` (`common.py:35-60`), if `PREPEND_WWW` is set and the host lacks `www.`, the same two helpers are used. The slash is added in that one redirect (`common.py:56-58`), so the client isn't redirected twice.

**Related**

`BrokenLinkEmailsMiddleware` (`common.py:182`) also checks `APPEND_SLASH`. It ignores 404s where the referer equals the current URL minus its trailing slash.

**Uncertainty**

I read the middleware and the request path code. I didn't open `is_valid_path` in `django/urls`, so its exact resolution behavior is not verified.