**Answer:** PR #3866 (merged 2026-09-23 into `main`, opening 8.6.0) deprecates two things. Both now emit a `DeprecationWarning`, and the first becomes a `TypeError` in Click 9.0. The PR prepares the ground for #3827. I did not read the last part of the test patches, which were paginated.

**1. Parameter names that are not usable Python names.** This applies to `Option` and `Argument`.
- Keywords trigger a warning. `click.option("--from")` names the parameter `from`, and `keyword.iskeyword` catches it.
- Names that fail `str.isidentifier()` trigger a warning. Examples are `click.argument("0foo")`, `--0-file`, names with a dot such as `foo.bar`, and names with a space.
- The empty name `""` also triggers a warning. This covers an `Argument([], expose_value=False)` and an option whose name can't be derived when `expose_value=False`.
- Soft keywords like `match` and `type` are not affected. `--True` and `--None` lower-case out of the keyword set, so they don't warn either.
- The check is `Parameter._check_name_is_usable` (core.py, patch hunk near the `_parse_decls` overloads). The warning text says "This is deprecated and will raise a TypeError in Click 9.0."
- It is called from `Option._parse_decls` and `Argument._parse_decls`, including the `expose_value=False` early returns.

**2. Explicit `Option` names that are not already lower-case.**
- `click.option("--x", "Foo_Bar")` currently names the parameter `Foo_Bar`. Click 9.0 will lower-case it to `foo_bar`.
- The check is `Option._check_name_is_normalized`. `_parse_decls` records `explicit_name` and calls the check when it is set.
- The migration is to spell the name the way the callback declares it.
- This applies to options only. `Argument` already lower-cases its declaration, so `Argument(["Foo_Bar"])` gives `foo_bar` with no warning (test `test_argument_normalizes_an_identifier_decl`).

**Migration, from the upgrade guide:**
- For options, pass an explicit name: `click.option("--from", "source")`.
- For arguments, rename and use `metavar`: `click.argument("zero_file", metavar="0-FILE")`.

**Supporting details:**
- The warnings use `stacklevel=_outside_click_stacklevel()`, so they point at the caller's code rather than Click internals.
- The `CHANGES.md` entries and the `docs/upgrade-guides.md` text confirm the scope. Both cite `{pr}3866`.

**Evidence:** PR #3866 patches for `src/click/core.py`, `CHANGES.md` and `docs/upgrade-guides.md`. I have hunk-level diff context, not pinned file line numbers.