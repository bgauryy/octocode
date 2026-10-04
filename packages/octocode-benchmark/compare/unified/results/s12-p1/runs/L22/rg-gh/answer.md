Two places in `django/` call `django.templatetags.static.static()`, the function that turns a relative asset path into a URL. It is defined at `django/templatetags/static.py:174`. Both callers import it directly.

1. **`django/forms/widgets.py`**
   - It imports the function at line 13: `from django.templatetags.static import static`.
   - `Media.absolute_path` calls it at line 228: `return static(path)`. The docstring at line 224 says the path "will be passed to django.templatetags.static.static()".
   - It is also called at line 121, in `return static(self._path)`.
2. **`django/contrib/admin/templatetags/admin_list.py`**
   - It imports the function at line 24.
   - It calls it at line 187: `icon_url = static(`.

The other two `static` functions are separate:
- The URL-pattern helper is `django/conf/urls/static.py:10`. `django/contrib/staticfiles/urls.py:2,14` calls it, not the asset-URL function.
- The context processor is `django/template/context_processors.py:76`. I found no direct callers of it.

My search was for `static(`, `def static` and `import ... static`. The command that looked for other import styles (`templatetags.static`, `templatetags import`) printed nothing. That could mean no matches, but the shell was also printing `/dev/null: Operation not permitted` errors. I therefore can't rule out a caller reached by an unusual import, such as a multi-line import or a `static.static` call.