**Root cause:** `ConfigWrapper.core_config()` wrote derived alias settings into the model's own config dict. Because of that write, a derived value looked like a declared one and was inherited by subclasses.

- `core_config()` set `validate_by_alias=True` and `validate_by_name=populate_by_name` on `self.config_dict`. That dict is the model's `model_config`, or the caller's mapping for `TypeAdapter(config=...)` and `validate_call(config=...)`. The old code is in the `-` lines of the PR diff, in `pydantic/_internal/_config.py`.
- A subclass's config is merged over its parent's. So a parent declaring `populate_by_name=True` passed its derived `validate_by_name=True` down to the child.
- The `validate_by_name is None` guard then blocked re-deriving from the child's own `populate_by_name=False`. The child ended up with `validate_by_name=True` while a standalone model with the same config had `False`.
- The result was that `Child(x=1)` succeeded, although the field's alias `'X'` should have been required (the issue's example, issue #13786).
- The same write also changed user-owned mappings. The issue shows `{'populate_by_name': True}` gaining the two derived keys after `TypeAdapter` and `validate_call`.

**Fix:** PR #13825, "Do not mutate `model_config` attribute". It was merged on 2026-09-25 as `5da36b5de4f44a572ca5c12104fd2f8669fd feca`, and its description says "Fixes #13786".

- **Effective config:** `ConfigWrapper` now has a second slot, `effective_config`. It is built once in `__init__` by a new `_build_effective_config()`, which derives the alias settings on a copy (`{**config, ...}`). The `validate-by-alias-and-name-false` check moved into this function.
- **Declared config:** `config_dict` stays as the user declared it and is documented as never to be mutated. Both `__getattr__` and `core_config()` now read `effective_config`.
- **Copy on entry:** `prepare_config()` now calls `config.copy()`, so the wrapper never mutates a mapping the user passed in.
- **Deprecation warning:** the `schema_generator` warning moved from `core_config()` to `check_deprecated()`.
- **Signatures:** the `validate_by_name` parameter was removed from signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`. Signatures now use only `validate_by_alias`.
- **Tests:** tests were added or changed in `tests/test_config.py`, `test_model_signature.py` and `test_deprecated_validate_arguments.py`.

**Behaviour change:** `Model.model_config` for `{'populate_by_name': True}` no longer includes `validate_by_alias` and `validate_by_name`. The PR description says this needs a blog post entry.

**Caveats:**
- I read the PR diff only up to the `_signature.py` changes. I did not read the rest of the diff (`pydantic/config.py`, `deprecated/decorator.py` and the tests), and I did not check the merged code on `main` or the test contents.
- Two earlier PRs for the same issue, #13787 and #13794, were closed without merging. #13787 was probably the issue author's own patch that copied the config inside `core_config`, but I didn't confirm that.