**Answer:** PR #3866 (merged 2026-09-23, for Click 8.6.0) adds `DeprecationWarning`s for parameter names that Click 9.0 will refuse or spell differently. It doesn't change behavior yet. Its stated purpose is to prepare for #3827 and to lock down the current name behavior with tests. This is based on the PR body, `CHANGES.md`, the upgrade guide and the `core.py` patch. I didn't read the test patches past the first page.

**What is deprecated** (`CHANGES.md`, `docs/upgrade-guides.md`):
1. A parameter name that isn't a valid Python identifier, or that is a Python keyword. Click 9.0 will raise `TypeError` for these.
2. An `Option` name written as a Python identifier that isn't already lower-case. Click 9.0 will lower-case it.

**Declarations that now warn** (`src/click/core.py`):
- **Non-identifier names** (`Parameter._check_name_is_usable`, added in the `_parse_decls` area around line 2479):
  - `click.argument("0foo")` and `click.option("--0-file")` warn "which is not a valid Python identifier". Other examples in the tests are `foo.bar`, `foo bar` and the empty name.
  - An unexposed parameter (`expose_value=False`) with no usable name also warns, including `Argument([], expose_value=False)`. This check is called inside the `not expose_value` branches of both `Option._parse_decls` and `Argument._parse_decls`.
- **Keyword names:**
  - `click.option("--from")` warns "which is a Python keyword".
  - Soft keywords such as `match` and `type` don't warn. `--True` and `--None` don't warn either, because they lower-case out of the keyword set.
- **Explicit option names that aren't lower-case** (`Option._check_name_is_normalized`): `click.option("--x", "Foo_Bar")` warns that Click 9.0 will name it `foo_bar`. The check only applies to the explicit-name declaration (`explicit_name`) in `Option._parse_decls`. Arguments aren't affected, because they already lower-case their declaration. The test `test_argument_normalizes_an_identifier_decl` shows `Argument(["Foo_Bar"]).name == "foo_bar"`.

**Details:**
- The warning is a `DeprecationWarning`. Its `stacklevel` comes from a new helper, `_outside_click_stacklevel()`, so it points at the user's code rather than Click's.
- Migration: pass an explicit name to an option, for example `click.option("--from", "source")`. An argument takes only one declaration, so rename it and use `metavar` to keep the old display, for example `click.argument("zero_file", metavar="0-FILE")`.

**Uncertainty:** The test patches were truncated (`isPartial`). I didn't see `tests/test_options.py` or `tests/test_deprecations.py`, and I didn't read the final lines of `core.py` beyond the shown hunks.