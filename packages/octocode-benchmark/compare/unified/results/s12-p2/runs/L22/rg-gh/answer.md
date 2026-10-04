The `static` that turns a relative asset path into a URL is `django.templatetags.static.static` (`django/templatetags/static.py:174`). It returns `StaticNode.handle_simple(path)`. Two places in the `django` package import and call it:

1. **`django/forms/widgets.py`**
   - It imports `static` at line 13.
   - `Media.absolute_path` calls `static(path)` at line 228. The docstring at lines 222-224 says it uses `django.templatetags.static.static()`.
   - The call at line 121 is in a method I didn't open, so I don't know its name. It is `return static(self._path)`.
2. **`django/contrib/admin/templatetags/admin_list.py`**
   - It imports `static` at line 24.
   - It calls `static(` at line 187, where it builds `icon_url`.

The other two functions named `static` are separate:
- **URL-pattern helper:** `django/conf/urls/static.py:10`. `django/contrib/staticfiles/urls.py:14` calls it with `static(prefix, view=serve)`.
- **Context processor:** `django/template/context_processors.py:76`.

`django/contrib/staticfiles/views.py:13` runs `from django.views import static`. That is a module, not this function.

**Uncertainty:** I searched `.py` files under `django/` with a grep for `static(`, `import ... static` and `static as`. A caller that reaches the function by another route would not show up. Examples are a call through a module attribute, or `StaticNode.handle_simple` called directly. I didn't check for those. I also didn't open the line 187 call, so I'm assuming it is the call to this `static`.