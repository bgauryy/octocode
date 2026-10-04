PR #3866 (merged 2026-09-23, merge commit `06b2a67`) deprecates certain parameter names. It adds `DeprecationWarning`s for names that Click 9.0 will refuse or spell differently. The PR also opens the 8.6.0 release on `main`. I read the PR body and the `CHANGES.md` and `src/click/core.py` diffs. I did not read the final merged line numbers, the `Argument` or `Parameter` code paths, or the tests.

**What is deprecated** (PR body, `CHANGES.md` hunk):
- A parameter name that is not a valid Python identifier. This raises `TypeError` in Click 9.0.
- A parameter name that is a Python keyword. This also raises `TypeError` in Click 9.0.
- An `Option` name written as a Python identifier that is not already lower-cased. Click 9.0 will lower-case it.

**Which declarations now warn:**
- **Not an identifier, or a keyword** (`Parameter._check_name_is_usable` in `core.py`):
  - The check uses `keyword.iskeyword`, then `str.isidentifier`. For example, `0-file` is not an identifier, and `--from` gives the keyword name `from`.
  - Soft keywords such as `match` and `type` do not warn.
  - The message reads "uses 'X' as its name, which is a Python keyword / not a valid Python identifier… will raise a TypeError in Click 9.0."
- **Options with an uppercase explicit name** (`Option._check_name_is_normalized`):
  - For example, an explicit identifier decl like `"Name"` warns that Click 9.0 will name `name` instead.
  - `Option._parse_decls` now records the identifier decl as `explicit_name`, which is what gets this check.
- **Options with no usable derived name** (`Option._parse_decls`):
  - The old code silently set `name = None` when a derived name was not an identifier. The new code keeps the name and goes to the `name is None or not name.isidentifier()` branch.
  - With `expose_value=False`, it now calls `_check_name_is_usable` and warns instead of staying silent.
  - With `expose_value=True`, it still raises `TypeError("Could not determine name…")`, as before.
- **Stack level:** the helper `_outside_click_stacklevel()` makes the warning point at the caller's code rather than at Click's internals.

**Uncertainty:** the patch output I saw was cut off after the `Option._parse_decls` hunk. I did not see where `_check_name_is_usable` is called for `Argument` and plain `Parameter`, or where `_check_name_is_normalized` is called. The PR also touches `tests/test_arguments.py`, `tests/test_options.py`, `tests/test_deprecations.py` and `docs/upgrade-guides.md`.