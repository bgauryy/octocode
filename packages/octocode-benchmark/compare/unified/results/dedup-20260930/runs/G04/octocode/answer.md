**Answer:** PR #3866 (merged, by kdeldycke) deprecates parameter names that Click 9.0 will refuse. It adds `DeprecationWarning`s in `src/click/core.py`, and Click 9.0 will raise `TypeError` for the first two cases below. The PR also starts the 8.6.0 release on `main`.

**Declarations that now warn:**
1. **Name is a Python keyword.** Example: `--from` gives the name `from`. `keyword.iskeyword` decides this. Soft keywords such as `match` and `type` do not warn. The check is in `Parameter._check_name_is_usable` and says "which is a Python keyword".
2. **Name is not a valid Python identifier.** Example: `0-file`. `str.isidentifier` decides this. The warning says "which is not a valid Python identifier".
   - `Parameter` base class: `_check_name_is_usable` is defined there and is called from the `Option` and `Argument` `_parse_decls`.
   - `Option`: the check runs at the end of `_parse_decls`. It also runs when the name is unusable and `expose_value=False`, which previously returned `""` silently. The old code set `name = None` for a non-identifier derived name. That is replaced by the `name is None or not name.isidentifier()` condition.
   - `Argument`: the check runs with its name, and with `""` when `decls` is empty and `expose_value=False`.
3. **`Option` with an explicit name that is not lower-cased.** Example: a bare-identifier decl like `"Name"`. `Option._check_name_is_normalized` compares `name.lower()` with `name`. It only covers a name written explicitly as a Python identifier (tracked via `explicit_name`). Names derived from `--flags` are already lower-cased. The warning says Click 9.0 will lower-case it.

All of these use `stacklevel=_outside_click_stacklevel()`, a new helper that points the warning at the first frame outside Click. The CHANGES.md entries are under "Version 8.6.0". The PR also adds tests in `tests/test_arguments.py`, `tests/test_deprecations.py` and `tests/test_options.py`, and a two-line change to `docs/upgrade-guides.md`.

**Caveats:**
- I did not read the tests or the docs diff.
- I did not check whether the code later changed on `main`.
- Line numbers are not cited because the patch view doesn't give them reliably. The evidence is the PR diff at merge commit `06b2a678741131fd577ce170e23e5ca0aeba0309`.
- The CHANGES text says the lower-casing deprecation applies to names "written as a Python identifier".
- The CHANGES entry for the first two cases says `TypeError` in Click 9.0. The `Option` lower-casing warning only says Click 9.0 will lower-case the name.