**Answer:** PR #3866 (merged, by kdeldycke) deprecates parameter names that Click 9.0 will refuse or spell differently. The affected names are ones that are not valid Python identifiers, are Python keywords, or are not lower-cased. Each case emits a `DeprecationWarning`. The PR also starts the 8.6.0 section of the changelog (`CHANGES.md`). It prepares for #3827.

**What now warns** (`src/click/core.py`, from the PR diff; I didn't read the merged file, so I have no line numbers):

1. **Keyword names, for `Option` and `Argument`.** `Parameter._check_name_is_usable` warns when `keyword.iskeyword(name)` is true. An example is `--from`, which names a parameter `from`. The message says "which is a Python keyword". Soft keywords such as `match` and `type` do not warn.
2. **Non-identifier names, for `Option` and `Argument`.** The same method warns when `not name.isidentifier()`, for example `0-file`. The message says "which is not a valid Python identifier". Both cases say the name "will raise a TypeError in Click 9.0".
   - `Option._parse_decls` calls the check on the derived name.
   - It also calls the check when `expose_value=False` and the name can't be derived. In that case an empty name is checked, so it warns.
   - `Argument._parse_decls` calls it on the argument name. It also calls it with an empty name when there are no decls and `expose_value=False`, so that case warns too.
3. **Explicit option names that are not lower-cased, `Option` only.** `Option._check_name_is_normalized` warns when a bare-identifier decl differs from its `.lower()`. An example is `click.Option(["Foo"])`. The message says Click 9.0 will lower-case it. Names derived from `--flag` style declarations were already lower-cased, so they don't trigger this.

The warnings use `stacklevel=_outside_click_stacklevel()`, so they point at the caller's code rather than Click internals.

**Tests and docs:** the PR adds about 770 lines of tests across `test_arguments.py`, `test_options.py` and `test_deprecations.py`. It also adds a note to `docs/upgrade-guides.md`, whose content I didn't read.

**Uncertainty:** I haven't confirmed how the keyword and non-identifier cases behave for a plain `Parameter` subclass.