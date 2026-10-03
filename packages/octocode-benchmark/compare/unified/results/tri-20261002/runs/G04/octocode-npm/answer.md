PR #3866 (merged 2026-09-23, targeting `main`, for the 8.6.0 cycle) deprecates certain parameter names. Click 9.0 will refuse them with a `TypeError`. Until then, 8.6 keeps them working and emits a `DeprecationWarning` (`src/click/core.py`, `_check_name_is_usable` and `_check_name_is_normalized`; `CHANGES.md`).

**What is deprecated**
1. A parameter name that is not a valid Python identifier, checked with `str.isidentifier()`.
2. A parameter name that is a Python keyword, checked with `keyword.iskeyword()`.
3. An `Option` name written as a Python identifier that is not already lower-case. Click 9.0 will lower-case it.

**Declarations that now warn**
- **Not an identifier** (`Parameter`, `Argument` and `Option`; message "…which is not a valid Python identifier"):
  - `Argument` declarations such as `"0foo"`, `"foo.bar"`, `"foo bar"` and `""`.
  - Any unexposed parameter (`expose_value=False`) whose name is not an identifier, such as `Argument(["0foo"], expose_value=False)` or `Option(["--0foo"], expose_value=False)`.
  - Unexposed parameters with no declarations at all, such as `Argument([], expose_value=False)` or `Option([], expose_value=False)`. These get the empty name `""`.
  - An exposed `Option` with such a name already raised `TypeError("Could not determine name")`, so it is unchanged.
  - Source: `tests/test_deprecations.py`.
- **Keyword** (message "…which is a Python keyword"): `Option(["--from"])`, `Option(["--import"])` and `Argument(["class"])`. `isidentifier()` accepts keywords, so these named a parameter today that no callback could declare.
- **Explicit option name not lower-case** (message "lower cases an explicit name"): `Option(["--x", "Foo_Bar"])`, `"X_Y"`, `"ΟΔΟΣ"` and the Kelvin sign. The name is kept as written for now, and 9.0 will lower-case it.

**What does not warn** (`tests/test_deprecations.py`, `test_usable_name_not_deprecated` and `test_normalized_explicit_name_not_deprecated`)
- Hyphens and mixed-case derived names, such as `Argument(["foo-bar"])`, `Argument(["Foo_Bar"])`, `Option(["--foo-bar"])` and `Option(["--Foo-Bar"])`.
- `Option(["--0foo", "zero_foo"])`, because an explicit valid name is the migration path.
- Soft keywords such as `match` and `type`.
- `--True` and `--None`, which lower-case out of the keyword set.
- Explicit names that are already lower-case, such as `"foo_bar"`, `"_from"` or `"café"`.

**Migration**
- Pass an explicit name, for example `click.option("--from", "source")`.
- For arguments, which take one declaration, rename the argument and use `metavar` to keep the old display: `click.argument("zero_file", metavar="0-FILE")` (`docs/upgrade-guides.md`).

**Caveat:** the patch output was truncated at 12,000 of 12,654 characters. The truncated remainder is in `tests/test_arguments.py`, so the picture above is not affected.