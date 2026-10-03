**PR #3866** ("Deprecate parameter names that are: not a Python identifier, or a Python Keyword, or not lower-cased", merged as 06b2a678) adds `DeprecationWarning`s for parameter names that Click 9.0 will refuse or spell differently. It also starts the 8.6.0 version on `main`. It is groundwork for #3827. The code is in `src/click/core.py` and the notes in `CHANGES.md` and `docs/upgrade-guides.md`.

**What is deprecated**
1. A parameter name that is not a valid Python identifier. It is checked with `str.isidentifier`. In 9.0 it raises `TypeError`.
2. A parameter name that is a Python keyword. It is checked with `keyword.iskeyword`. In 9.0 it raises `TypeError`.
   - Soft keywords such as `match` and `type` are not affected.
3. An `Option` name written as a Python identifier that is not already lower-cased. Click 9.0 will lower-case it.

**Declarations that now warn**

The first two are checked by `Parameter._check_name_is_usable`. It is called from `Option._parse_decls` and `Argument._parse_decls`. The third is checked by `Option._check_name_is_normalized`.

- **Non-identifier names**
  - `click.argument("0foo")` and `click.option("--0-file")` warn, because the derived name `0_file` is not an identifier.
  - The message reads "…which is not a valid Python identifier".
  - This also covers `expose_value=False` parameters where no name can be derived. The check runs with `""`, for both Option and Argument with no decls.
- **Keyword names**
  - `click.option("--from")` warns, because the name `from` is a keyword.
  - The message reads "…which is a Python keyword".
  - `--True` and `--None` do not warn, because they are lower-cased to `true` and `none`, which are not keywords.
- **Upper-case explicit option names**
  - `click.option("--x", "Foo_Bar")` warns, because it names `foo_bar` rather than `Foo_Bar`.
  - Only a bare identifier decl counts as an explicit name. A derived name is already lower-cased, so it never triggers this.
  - This case is Option-only.

**Migration** (from `docs/upgrade-guides.md`)
- For options, pass an explicit name: `click.option("--from", "source")`.
- For arguments, rename them and use `metavar`: `click.argument("zero_file", metavar="0-FILE")`.

**Uncertainty:** I read this from the PR diff and metadata through `gh`, not from a local checkout, so I have no stable line numbers to cite. The upgrade guide has no PR link for the lower-casing bullet other than #3827, and I did not run the tests.