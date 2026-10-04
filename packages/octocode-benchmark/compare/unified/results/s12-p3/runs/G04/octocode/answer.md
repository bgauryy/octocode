**PR #3866 (merged 2026-09-23, targets `main`, opens 8.6.0)** adds `DeprecationWarning`s for parameter names that Click 9.0 will refuse or spell differently. It is groundwork for #3827. It doesn't change behavior yet.

**What is deprecated** (`CHANGES.md` in the PR; `src/click/core.py`, `_check_name_is_usable` and `_check_name_is_normalized`):
1. A parameter name that is not a valid Python identifier, or that is a Python keyword. This will raise `TypeError` in 9.0.
2. An `Option` name written as a Python identifier that is not already lower-cased. 9.0 will lower-case it.

**Declarations that now warn:**
- **Non-identifier names** (checked with `str.isidentifier()`), for both `Argument` and `Option`:
  - `click.argument("0foo")` and `click.option("--0-file")`.
  - Names containing a dot or a space, such as `foo.bar` or `foo bar`.
  - Per the new tests, an unexposed argument still keeps the name and warns, for example `Argument(["0foo"], expose_value=False)`. `Argument([], expose_value=False)` also warns, with name `""`.
- **Keyword names** (checked with `keyword.iskeyword`): `click.option("--from")` names the parameter `from`, which no callback can declare. Soft keywords such as `match` and `type` are unaffected. `--True` and `--None` lower-case out of the keyword set, so they don't warn.
- **Explicit option names that aren't lower-case:** `click.option("--x", "Foo_Bar")` currently names the parameter `Foo_Bar`. 9.0 will name it `foo_bar`. This check applies only to an explicit identifier name, tracked in `explicit_name` in `Option._parse_decls`.

Arguments are not covered by the casing warning. They already normalize, so `Argument(["Foo_Bar"]).name` is `foo_bar` with no warning.

**Warning details:**
- The category is `DeprecationWarning`.
- The stack level points at the first frame outside Click.
- The messages end with "This is deprecated and will raise a TypeError in Click 9.0" for the name checks. The casing message says "Click 9.0 lower cases an explicit name".

**Migration** (`docs/upgrade-guides.md`):
- For options, pass an explicit name: `click.option("--from", "source")`.
- For arguments, rename the argument and pass `metavar` to keep the old display: `click.argument("zero_file", metavar="0-FILE")`.
- For the casing warning, spell the name the way the callback declares it.

**Uncertainty:** the diff output was truncated. I did not see the rest of the test changes in `tests/test_arguments.py`, nor `tests/test_deprecations.py` and `tests/test_options.py`. The `core.py` and docs hunks I read were complete, though the tool elided some unchanged context lines.