**PR #3866 (merged 2026-09-23, merge commit `06b2a678`) deprecates three kinds of parameter name.** It prepares for #3827, which will make Click 9.0 refuse them. Each one now emits a `DeprecationWarning`. I read the PR body and the `core.py` and `CHANGES.md` diff, and I did not run the code.

**What is deprecated**
1. A name that is a Python keyword, such as `from`. `_check_name_is_usable` tests it with `keyword.iskeyword`. The warning says it "will raise a TypeError in Click 9.0".
2. A name that is not a valid Python identifier, such as `0-file`. The same method tests it with `str.isidentifier`. A name gets one reason or the other, never both.
3. An explicit `Option` name that is not already lower-case. `Option._check_name_is_normalized` warns that Click 9.0 will lower-case it and use that spelling instead.

Soft keywords such as `match` and `type` do not warn, because `keyword.iskeyword` passes them.

**Which declarations warn**
- **`Option`:**
  - Explicit bare-identifier names warn if they are mixed-case, for example `"Name"` in `@click.option("--x", "Name")`.
  - Explicit names also warn if they are keywords. The identifier check does not apply to them, since they already pass `isidentifier`.
  - A name derived from the flags warns in two cases. One is a keyword, such as `--from`, which gives `from`. The other is a non-identifier, such as `--0-file`.
  - Derived names are already lower-cased, so they never trigger the lower-case warning.
  - The `expose_value=False` path also checks the name, using `name or ""`. An option with no usable name there now warns instead of failing silently.
- **`Argument`:**
  - A name that is a keyword or a non-identifier warns.
  - An argument with no decls and `expose_value=False` warns with an empty name, `""`.
  - There is no lower-case check for arguments. In the diff, `_check_name_is_normalized` is only added to `Option`.

**Details**
- The warning's `stacklevel` comes from `_outside_click_stacklevel()`, so it points at the caller's code instead of Click's internals.
- `CHANGES.md` lists this under a new "Version 8.6.0" heading. The PR also changed `docs/upgrade-guides.md`, `pyproject.toml`, `uv.lock` and the tests (`test_arguments.py`, `test_options.py`, `test_deprecations.py`).

**Not verified**
I did not read the tests or the upgrade-guide text. I also did not check whether the stated behavior still matches `main`.