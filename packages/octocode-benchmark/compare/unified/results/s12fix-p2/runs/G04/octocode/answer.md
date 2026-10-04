**PR #3866 (merged 2026-09-23, targets `main`, 8.6.0)** adds `DeprecationWarning`s. It deprecates two things, ahead of Click 9.0 and #3827. The patches were truncated in my read, so the test files and `uv.lock` are not fully reviewed. The `src/click/core.py` and `CHANGES.md` diffs were complete.

**1. Unusable parameter names (`TypeError` in Click 9.0).** This applies to both `Option` and `Argument`. It is checked by `_check_name_is_usable` in `src/click/core.py`. The warning fires when the resulting parameter name is:
- **A Python keyword**, per `keyword.iskeyword`. For example, `click.option("--from")` names a parameter `from`. Soft keywords such as `match` and `type` are not affected. `--True` and `--None` lower-case out of the keyword set.
- **Not a valid identifier**, per `str.isidentifier()`. Examples are `click.argument("0foo")` and `click.option("--0-file")`. Other cases include names with a dot or a space.

The warning also fires when `expose_value=False` and the name is empty. That covers `Argument([], expose_value=False)` and an option whose name can't be derived. An exposed argument with no declaration still raises `TypeError` ("does not have a name").

**2. Explicit option names that aren't lower-case.** This applies to `Option` only. It is checked by `_check_name_is_normalized`. It fires when a non-declaration-style explicit name (one without dashes or slashes) is a valid identifier but differs from `name.lower()`. For example, `click.option("--x", "Foo_Bar")` currently names the parameter `Foo_Bar`, and Click 9.0 will name it `foo_bar`. Arguments are not affected, because they already lower-case their declaration, so `click.Argument(["Foo_Bar"]).name == "foo_bar"`.

**Migration** (from `docs/upgrade-guides.md`):
- Pass an explicit name: `click.option("--from", "source")`.
- For arguments, rename the declaration and keep the old display with `metavar`: `click.argument("zero_file", metavar="0-FILE")`.
- Spell option names in lower case.

**Details:**
- The warnings use `stacklevel=_outside_click_stacklevel()`, so they point at the caller's code rather than Click's.
- The `core.py` changes also set `explicit_name` in `Option._parse_decls`.
- The PR adds tests that pin the naming behavior, in `tests/test_arguments.py` and elsewhere.