**PR #3866 deprecates parameter names that Click 9.0 will refuse.** It emits a `DeprecationWarning` for each case below. The PR title is "Deprecate parameter names that are: not a Python identifier, or a Python Keyword, or not lower-cased". The change is in `src/click/core.py`, and `CHANGES.md` has the 8.6.0 entries.

**Declarations that now warn**
1. **A name that is a Python keyword**, such as `from`. This is checked in `Parameter._check_name_is_usable` with `keyword.iskeyword`, and an example is `--from`. The message says the name "is a Python keyword". Soft keywords such as `match` and `type` don't warn.
2. **A name that is not a valid Python identifier**, such as `0-file`. This is checked with `str.isidentifier`. The message says the name "is not a valid Python identifier". Click 9.0 will raise `TypeError` for both of these first two cases.
3. **An `Option` whose explicit name is written as an identifier but isn't lower-cased**, such as `MyName`. This is checked in `Option._check_name_is_normalized`, which compares `name.lower()` to `name`. Click 9.0 will lower-case it, so the warning says it will name `'myname'` instead.

**Details**
- The PR description says `Parameter`, `Argument` and `Option` are all covered. The `Option` hunks are the ones I saw in full, and I didn't confirm the `Argument` call sites.
- In the `_parse_decls` hunk I read, `Option` calls `_check_name_is_usable` when the derived name is not an identifier and `expose_value` is false. I didn't see the rest of the diff, so I can't say where the other call sites are.
- The `Option._parse_decls` hunk now records `explicit_name` when a declaration is itself an identifier. Auto-derived names from `--foo-bar` are lower-cased already, so they don't trigger the third warning.
- Warnings use `stacklevel=_outside_click_stacklevel()`, so they point at the user's code rather than Click internals.
- The PR is a preparation step for #3827, which will enforce the refusal. It also opens the 8.6.0 release on `main`. The merge commit is `06b2a678741131fd577ce170e23e5ca0aeba0309`.

**Uncertainty:** I read only the first ~150 lines of the diff, truncated, and I didn't check the tests or line numbers at the pinned commit. For that reason I'm citing files and function names here, not `path:line`.