**PR #3866** (merged 2026-09-23, merge commit `06b2a67`) deprecates parameter names that Click 9.0 will refuse or spell differently. It does this with `DeprecationWarning`s. Click 9.0 is meant to raise `TypeError` for the first two cases (via the follow-up #3827). I read the PR description, the `CHANGES.md` hunk and the `core.py` diff. I did not read the test files, and I did not check the parts of the `Argument` and `Parameter` code that the diff output cut off.

**What is deprecated**
1. A parameter name that is not a valid Python identifier, such as `0-file`. The check is `str.isidentifier`.
2. A parameter name that is a Python keyword, such as `from`. The check is `keyword.iskeyword`. Soft keywords such as `match` and `type` do not warn.
3. An `Option` name written as a Python identifier that is not already lower-cased. Click 9.0 will lower-case it.

**Which declarations warn**
- **`Parameter._check_name_is_usable`** (added in `src/click/core.py`) covers cases 1 and 2. The message reads "Parameter/Option/Argument [decls] uses 'x' as its name, which is a Python keyword / not a valid Python identifier… will raise a TypeError in Click 9.0."
  - In `Option._parse_decls`, the old fallback that set `name = None` when the derived name was not an identifier is gone. A derived name that is not an identifier, such as one from `--0-file`, now reaches the check when `expose_value=False`. With `expose_value=True` it still raises `TypeError` ("Could not determine name…"), as before.
  - The PR description says `Argument` and `Parameter` are also covered, but I only saw the `Option` call site in the diff. A keyword name such as `--from` is also covered by this check, but I did not see where the check is called for it.
- **`Option._check_name_is_normalized`** covers case 3. It warns only for an explicit identifier-style declaration that is not lower-cased, for example `click.Option(["Foo", "--bar"])` or `["FOO"]`. The PR tracks this through a new `explicit_name` variable in `Option._parse_decls`. Names derived from `--long-opt` flags were already lower-cased, so they do not warn.

Both warnings use `stacklevel=_outside_click_stacklevel()`, a new helper that points the warning at the first stack frame outside Click.

The PR also touches `docs/upgrade-guides.md`, `pyproject.toml`, `uv.lock` and three test files (`tests/test_arguments.py`, `tests/test_deprecations.py`, `tests/test_options.py`). It starts the 8.6.0 section of `CHANGES.md`.