The function you want is `django.templatetags.static.static(path)`, defined at `django/templatetags/static.py:174`. Three places in the `django` package call it directly.

- **`django/forms/widgets.py:121`**: `return static(self._path)`. The file imports it at line 13 (`from django.templatetags.static import static`).
- **`django/forms/widgets.py:228`**: `return static(path)`. This is in the media path-to-URL method, and the docstring at line 224 names `django.templatetags.static.static()`.
- **`django/contrib/admin/templatetags/admin_list.py:187`**: `icon_url = static(`. The file imports it at line 24.

The other two functions named `static` are not callers of this one:
- `django/conf/urls/static.py:10` is the URL-pattern helper. It is used by `django/contrib/staticfiles/urls.py:14`, which imports it from `django.conf.urls.static`.
- `django/template/context_processors.py:76` is the context processor.

I searched only `.py` files under `django/` and did not open the call sites. I also did not check for indirect uses, such as `getattr` or a re-export, or for template-level `{% static %}` use. Templates in the package, such as the admin's, probably use the tag, which wraps the same logic, but I did not check that. I excluded tests by searching only `django/`, which does not include the top-level `tests/` directory.