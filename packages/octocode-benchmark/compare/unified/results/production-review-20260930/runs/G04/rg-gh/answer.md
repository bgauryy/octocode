**PR #3866 (merged, commit 06b2a678) adds `DeprecationWarning`s for parameter names that Click 9.0 will reject or spell differently.** Nothing is removed yet. It also starts the 8.6.0 cycle on `main`. It prepares the ground for #3827.

**Names that now warn**

1. **A name that isn't a valid Python identifier.** This is checked with `str.isidentifier()` in `Parameter._check_name_is_usable`, in `src/click/core.py` (added in the PR diff). Examples are `click.argument("0foo")` and `--0-file`. Click 9.0 will raise `TypeError` for these.
2. **A name that is a Python keyword.** This is checked with `keyword.iskeyword` in the same method. An example is `click.option("--from")`, which names the parameter `from`.
   - Soft keywords such as `match` and `type` are not affected.
   - `--True` and `--None` lower-case out of the keyword set, so they don't warn.
3. **An explicit Option name that is not lower-case.** This is checked in `Option._check_name_is_normalized`. An example is `click.option("--x", "Foo_Bar")`, which keeps the name `Foo_Bar`. Click 9.0 will lower-case it to `foo_bar`.

**Where the warnings fire**

- **`Option._parse_decls`:** it calls both checks. The usable-name check also runs on the `expose_value=False` path when the name can't be determined.
- **`Argument._parse_decls`:** it calls the usable-name check. This includes the empty-declaration case with `expose_value=False`.
- **`Argument`, lower-casing:** there is no lower-case warning for arguments. An `Argument` already lower-cases its declaration, so `Argument(["Foo_Bar"]).name == "foo_bar"`. An `Option` keeps an explicit name as written, which is why only options get the third warning.

**Migration advice**

- For options, pass an explicit name, for example `click.option("--from", "source")`.
- An argument takes only one declaration. Rename it and pass `metavar` to keep the old display, for example `click.argument("zero_file", metavar="0-FILE")`.

Both warnings use a custom stacklevel, so they point at the first frame outside Click (`_outside_click_stacklevel`). The PR also adds tests in `tests/test_arguments.py`, `tests/test_options.py` and `tests/test_deprecations.py`.

**Caveats**

- I read the PR metadata and diff through `gh`, not a checkout, so the cited locations are from the diff rather than line numbers at the pinned commit.
- I only saw the first 300 lines of the diff, so I didn't read the test files in full.
- The diff for `docs/upgrade-guides.md` (the 8.3.X upgrade-guide section) says "raises `TypeError` in 9.0". `CHANGES.md` says "Click 9.0". These are the same claim.