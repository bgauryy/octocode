The function you mean is `static(path)` in `django/templatetags/static.py:174`. Two other `static` functions exist and are not it: the URL-pattern helper `django/conf/urls/static.py:10` and the context processor `django/template/context_processors.py:76`.

Two places in the `django` package call it:

- **`django/forms/widgets.py`**
  - It imports the function at line 13 (`from django.templatetags.static import static`).
  - `return static(self._path)` at line 121.
  - `return static(path)` at line 228, inside the method whose docstring (line 224) says the path "will be passed to django.templatetags.static.static()".
- **`django/contrib/admin/templatetags/admin_list.py`**
  - It imports the function at line 24.
  - It calls `icon_url = static(` at line 187.

Other matches in the search were not calls to this function:
- `django/contrib/staticfiles/urls.py:2` imports and calls the URL-pattern helper from `conf.urls.static` (line 14).
- `django/contrib/staticfiles/views.py:13` imports `django.views.static`, which is a module.

I only searched `.py` files for `static(` and import patterns, and I didn't read the call sites beyond the matched lines. A call through an alias or a dynamic lookup would not have shown up.