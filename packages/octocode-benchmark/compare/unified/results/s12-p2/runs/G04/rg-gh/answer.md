PR #3866 (merged as `06b2a678`) deprecates parameter names that Click 9.0 will refuse. Click 9.0 will raise `TypeError` for names that aren't valid identifiers or are keywords. The PR also deprecates explicit option names that aren't lower-case. All three cases emit a `DeprecationWarning` today.

**Declarations that now warn** (from the PR's `src/click/core.py` diff and `tests/test_deprecations.py`):

1. **Name is not a valid Python identifier.** `str.isidentifier()` decides this, and the warning says "not a valid Python identifier".
   - Applies to `Parameter`, `Argument` and `Option`, via `_check_name_is_usable`.
   - Examples from the tests: `Argument(["0foo"])`, `Argument(["foo.bar"])`, `Argument(["foo bar"])`, `Argument([""])`, `Option(["--0foo"], expose_value=False)`, and `Option([], expose_value=False)` / `Argument([], expose_value=False)`.
2. **Name is a Python keyword.** `keyword.iskeyword` decides this, and the warning says "is a Python keyword".
   - Examples: `Option(["--from"])`, `Option(["--import"])`, `Argument(["class"])`.
   - Soft keywords such as `match` and `type` don't warn. `--True` and `--None` don't warn either, because the name is lower-cased first.
3. **Option with an explicit name that isn't lower-case.** This is checked in `Option._check_name_is_normalized`, and the warning text includes "lower cases an explicit name".
   - Example: `click.Option(["--x", "Foo_Bar"])` currently names the option `Foo_Bar`, but Click will lower-case it.
   - The upgrade guide describes this as an option name written as a Python identifier that isn't already lower-cased.

**Declarations that don't warn:** hyphenated or mixed-case implicit names such as `Argument(["foo-bar"])`, `Argument(["Foo_Bar"])` and `Option(["--foo-bar"])`. Click already normalizes these to `foo_bar`. An explicit valid name such as `Option(["--0foo", "zero_foo"])` is also fine.

**Migration** (from the PR's `docs/upgrade-guides.md` entry):
- For options, pass an explicit name: `click.option("--from", "source")`.
- For arguments, rename the argument and use `metavar` to keep the old display: `click.argument("zero_file", metavar="0-FILE")`.

**Other changes in the PR:**
- It bumps the version to 8.6.0.dev and adds a `CHANGES.md` entry.
- It adds tests that pin the existing naming behaviour.
- It prepares for #3827, which will enforce the refusal.

**Uncertainty:** the diff output was truncated, so I didn't see the full body of `_check_name_is_normalized` or its exact warning message. The wording above comes from the tests' `match=` strings.