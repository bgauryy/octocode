The function is `django.templatetags.static.static` (defined at `django/templatetags/static.py:174`). Code inside the `django` package calls it from two files. A text search of `django/*.py` found these.

- **`django/forms/widgets.py`**
  - It imports `static` at line 13.
  - `return static(self._path)` at line 121 (in the `Media` code).
  - `return static(path)` at line 228. The docstring at line 224 says the path "will be passed to django.templatetags.static.static()".
- **`django/contrib/admin/templatetags/admin_list.py`**
  - It imports `static` at line 24.
  - `icon_url = static(` at line 187.

The other two `static` functions are not part of this answer:
- The URL-pattern helper is `django/conf/urls/static.py:10`. It is called from `django/contrib/staticfiles/urls.py:14` (`return static(prefix, view=serve)`).
- The context processor is `django/template/context_processors.py:76`.

I did not trace other ways of reaching the function, such as `{% static %}` tags in templates or a call through the module path rather than a direct import. I also did not read the call sites beyond the matched lines.