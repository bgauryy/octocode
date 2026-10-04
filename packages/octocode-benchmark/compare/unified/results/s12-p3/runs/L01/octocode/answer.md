The redirect is done by `CommonMiddleware` in `django/middleware/common.py`, and it only happens after the original URL has returned a 404. I read this file at the pinned checkout. I did not open `is_valid_path`, `HttpRequest.get_full_path` or `escape_leading_slashes`, so what they do is taken from their names and how they're called.

1. **Trigger (`process_response`, lines 101–111).** The view is resolved and run first. If the response status is 404 and `should_redirect_with_slash(request)` returns true, the response is replaced. The replacement is `self.response_redirect_class(self.get_full_path_with_slash(request))` (lines 108–111). `response_redirect_class` is `HttpResponsePermanentRedirect` (line 33), so the redirect is a 301.

2. **Decision (`should_redirect_with_slash`, lines 63–75).** It returns true only when all of these hold:
   - `settings.APPEND_SLASH` is on and `request.path_info` doesn't end with `/` (line 68).
   - The path doesn't resolve as-is: `not is_valid_path(request.path_info, urlconf)` (line 70). `urlconf` comes from `request.urlconf` if set (line 69).
   - The path with `/` appended does resolve: `is_valid_path("%s/" % request.path_info, urlconf)` (line 71).
   - The matched view doesn't opt out. It returns `getattr(view, "should_append_slash", True)` (lines 73–74). A view can disable the redirect by setting `should_append_slash` to false on itself.

3. **Building the target (`get_full_path_with_slash`, lines 77–99).**
   - It calls `request.get_full_path(force_append_slash=True)` (line 84). That method is what puts the `/` before the query string.
   - It passes the result through `escape_leading_slashes` so the redirect can't become a scheme-relative URL (lines 85–86).
   - If `settings.DEBUG` is on and the method is DELETE, POST, PUT or PATCH, it raises a `RuntimeError` instead (lines 87–98). A redirect would lose the request body, so the error tells the developer to fix the form's URL or set `APPEND_SLASH=False`.

4. **Docstring summary (lines 19–25).** The class docstring says the same thing. The redirect is issued only if the URL with the slash appended exists in `urlpatterns`.