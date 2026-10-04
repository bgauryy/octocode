**PR #3866** (merged 2026-09-23, merge commit `06b2a678`) deprecates parameter names that Click 9.0 will refuse or spell differently. Each case emits a `DeprecationWarning`. The warning is raised for `Parameter`, `Argument` and `Option` names, in `src/click/core.py`. The PR calls this groundwork for #3827.

**Declarations that now warn**

1. **Name is not a valid Python identifier.** The check is `not name.isidentifier()` in `Parameter._check_name_is_usable`. Examples: `click.argument("0foo")` and `click.option("--0-file")`. The message says it "will raise a TypeError in Click 9.0".
2. **Name is a Python keyword.** The check is `keyword.iskeyword(name)` in the same method. Example: `click.option("--from")` names a parameter `from`. Soft keywords such as `match` and `type` are not affected. `--True` and `--None` are also fine, because they are lower-cased first.
3. **Option with an explicit name that is not lower-case.** This is `Option._check_name_is_normalized`, which only runs when a declaration is itself an identifier (`explicit_name`). Example: `click.option("--x", "Foo_Bar")`. Today that names `foo_bar`? No: it keeps `Foo_Bar`, and Click 9.0 will lower-case it to `foo_bar`.

**Where the checks run**
- **Options:** `Option._parse_decls` calls both checks. The usable-name check also runs when `expose_value=False` and no valid name could be derived.
- **Arguments:** `Argument._parse_decls` calls the usable-name check. It also runs on the empty-decls, `expose_value=False` path, with the name `""`.
- **Stack level:** the warnings use `_outside_click_stacklevel()`, so they point at the caller's code rather than Click's internals.

**Other changes**
- `CHANGES.md` starts a "Version 8.6.0" section.
- `docs/upgrade-guides.md` documents the deprecation and says to pass an explicit name, for example `click.option("--from", "source")`. Arguments take only one declaration, so the guide says to rename them and use `metavar`.
- Tests were added in `tests/test_deprecations.py`, `tests/test_arguments.py` and `tests/test_options.py`.

**Uncertainty**
- I read the diff only. I did not run the code or the tests.
- The third case above is where I second-guessed myself mid-answer. The diff and upgrade guide say the uppercase explicit name currently survives as written and will be lower-cased in 9.0. The upgrade guide's own example ("names `foo_bar` rather than `Foo_Bar`") reads the other way, so the exact current behavior is ambiguous to me.