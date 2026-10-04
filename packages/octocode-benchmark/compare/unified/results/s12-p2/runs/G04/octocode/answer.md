**PR #3866 (merged 2026-09-23, targets `main`, opens 8.6.0) adds `DeprecationWarning`s for parameter names that Click 9.0 will refuse or spell differently.** It changes no behavior in 8.x. The PR body says it prepares the ground for #3827 and copies that PR's tests to lock down the current `name` behavior.

**What is deprecated** (`CHANGES.md`, `docs/upgrade-guides.md`):
1. A parameter name that is not a valid Python identifier, or that is a Python keyword. This will raise `TypeError` in Click 9.0.
2. An `Option` explicit name written as a Python identifier that is not already lower-case. Click 9.0 will lower-case it.

**Declarations that now warn:**
- **Non-identifier names.** `_check_name_is_usable` in `core.py` warns when `str.isidentifier()` fails. Examples from the docs and tests:
  - `click.argument("0foo")` and `click.argument("0-file")`
  - `click.option("--0-file")`
  - names containing a dot or a space, such as `foo.bar` and `foo bar`
- **Keyword names.** The same check warns when `keyword.iskeyword(name)` is true, for example `click.option("--from")`, which names a parameter `from`.
  - Soft keywords such as `match` and `type` do not warn.
  - `--True` and `--None` do not warn, because they lower-case out of the keyword set.
- **Unexposed parameters.** The check also runs when `expose_value=False`:
  - `Option._parse_decls` calls it with `name or ""`.
  - `Argument._parse_decls` calls it with `""` when there are no decls, and the test expects `click.Argument([], expose_value=False)` to warn.
  - `click.Argument(["0foo"], expose_value=False)` also warns.
- **Non-lower-case explicit option names.** `Option._check_name_is_normalized` warns when `name.lower() != name`. The example is `click.option("--x", "Foo_Bar")`, which keeps the name `Foo_Bar` today but will become `foo_bar`.
  - Only a bare identifier passed as an explicit name triggers this (`explicit_name = decl`).
  - Arguments already lower-case their declaration, so `Argument(["Foo_Bar"])` becomes `foo_bar` without a warning (per `test_argument_normalizes_an_identifier_decl`).

**Migration** (from the upgrade guide):
- For options, pass an explicit name, e.g. `click.option("--from", "source")`.
- For arguments, rename the argument and use `metavar` to keep the old display, e.g. `click.argument("zero_file", metavar="0-FILE")`.

**Uncertainty:** The patch output was truncated. I did not read the rest of the test changes (`tests/test_arguments.py`, `test_options.py`, `test_deprecations.py`). The `core.py` hunks are shown with elided context, so I have no line numbers.