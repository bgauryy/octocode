PR #3866 (merged 2026-09-23, targeting `main`, which becomes 8.6.0) adds `DeprecationWarning`s only. It rejects nothing yet. It prepares for #3827, which will make Click 9.0 refuse these names. I read the PR body, `CHANGES.md`, `docs/upgrade-guides.md` and the `src/click/core.py` patch. The test patches were truncated, so I haven't read them in full.

**What is deprecated**

1. **Parameter names that are not valid Python identifiers, or are Python keywords.** Click 9.0 will raise `TypeError` for these. The check is `Parameter._check_name_is_usable` in `core.py`. It uses `keyword.iskeyword(name)` first, then `not name.isidentifier()`.
2. **Option names written as an identifier that is not already lower-cased.** Click 9.0 will lower-case an explicit name like any other declaration. The check is `Option._check_name_is_normalized`, which warns when `name.lower() != name`.

**Declarations that now warn**

- **Non-identifier names**, for both `Option` and `Argument`:
  - `click.argument("0foo")`
  - `click.option("--0-file")` (the derived name `0_file` isn't an identifier)
  - Names with a dot or a space, such as `foo.bar` or `foo bar`. The PR's tests cover these, but I only saw them in the truncated test patch.
- **Keyword names:** `click.option("--from")` names a parameter `from`, which `isidentifier()` accepts but no callback can declare. Soft keywords such as `match` and `type` don't warn. `--True` and `--None` lower-case out of the keyword set, so they don't warn either.
- **Unexposed parameters with an empty or invalid name** also warn. The check runs on the `expose_value=False` early-return paths for `Option` and for `Argument` (including `Argument([], expose_value=False)`, whose name is `""`).
- **Explicit option names that aren't lower-case:** `click.option("--x", "Foo_Bar")` currently names `Foo_Bar`, and Click 9.0 will name it `foo_bar`.
- **Arguments:** an `Argument` doesn't get the lower-casing warning. It already lower-cases its declaration, so `Foo_Bar` becomes `foo_bar` with no warning. Only the identifier/keyword warning applies to arguments.

**Migration** (from `docs/upgrade-guides.md`):
- For options, pass an explicit name, for example `click.option("--from", "source")`.
- For arguments, rename the declaration and pass `metavar` to keep the old display, for example `click.argument("zero_file", metavar="0-FILE")`.
- For explicit option names, spell them lower-case.

The warning text is "…uses 'X' as its name, which is a Python keyword / not a valid Python identifier. This is deprecated and will raise a TypeError in Click 9.0." It is emitted with a stacklevel that points at the first frame outside Click, via `_outside_click_stacklevel()`.