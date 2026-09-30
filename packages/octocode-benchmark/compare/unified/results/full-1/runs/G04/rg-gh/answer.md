**PR #3866 (merged, commit `06b2a678`) deprecates parameter names that Click 9.0 will refuse or spell differently.** It adds `DeprecationWarning`s now, and Click 9.0 will raise `TypeError` for the first two cases. It prepares the ground for #3827, and it also starts the 8.6.0 version on `main`.

**Declarations that now warn** (`src/click/core.py` and `CHANGES.md` in the PR diff):

1. **Name is not a valid Python identifier.**
   - `Parameter._check_name_is_usable` checks with `str.isidentifier()`.
   - Examples from the docs: `click.argument("0foo")`, or an option `--0-file` with no explicit name.
   - The warning text is "which is not a valid Python identifier".
2. **Name is a Python keyword.**
   - The same method checks with `keyword.iskeyword`.
   - Example: `click.option("--from")` names a parameter `from`.
   - Soft keywords such as `match` and `type` are unaffected. `--True` and `--None` are lower-cased out of the keyword set, so they don't warn.
3. **Option with an explicit identifier name that isn't lower-case.**
   - `Option._check_name_is_normalized` warns when `name.lower() != name`.
   - Example from the docs: `click.option("--x", "Foo_Bar")` currently names `foo_bar`, not `Foo_Bar`.
   - Click 9.0 will lower-case explicit names like any other declaration. The changelog lists only this case as "deprecated" without a `TypeError`.

**Where the checks run:**
- `Option._parse_decls` calls both checks.
- `Argument._parse_decls` calls the usable-name check. It also calls it with an empty name when there are no declarations and `expose_value=False`.
- `Option._parse_decls` also calls the usable-name check in the `expose_value=False` case.
- Warnings use a `stacklevel` that points at the first frame outside Click.

**Migration** (from `docs/upgrade-guides.md`):
- For options, pass an explicit name, for example `click.option("--from", "source")`.
- Arguments take only one declaration and have no explicit-name channel. Rename the argument and pass `metavar`, for example `click.argument("zero_file", metavar="0-FILE")`.

**Uncertainty:**
- I read the diff and PR metadata, not a checkout, and I didn't run the tests.
- Line numbers in the diff are relative to the hunks, so I haven't given file line numbers.
- The PR also adds tests in `tests/test_arguments.py`, `tests/test_options.py` and `tests/test_deprecations.py`. I didn't read them.