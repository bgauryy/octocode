**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings back into `self.config_dict`. That dict is the model's own `model_config`, or the mapping the caller passed to `TypeAdapter`, `validate_call` and similar.

- **The write:** when `populate_by_name` was set, it also stored `validate_by_alias=True` and `validate_by_name=<populate_by_name>`. It did the same for `validate_by_name=True` when `validate_by_alias=False`.
- **Why it leaks:** a subclass's config is merged over its parent's, so the parent's derived `validate_by_name=True` was inherited as if the user had declared it. The `if config.get('validate_by_name') is None` guard then stopped the child's own `populate_by_name=False` from being re-derived.
- **Effect:** `Child` (`populate_by_name=False`, alias `'X'`) silently accepted `x=1`, while an equivalent `Standalone` model correctly raised a `ValidationError`.
- **Side effect:** caller-supplied mappings were mutated too, as the issue shows for `TypeAdapter` and `validate_call`.

The issue text is the source for this diagnosis. I read the pre-fix code only through the PR diff.

**Fix:** PR #13825, merged 2026-09-25, closed the issue. PR #13794 was also linked as closing it but was closed rather than merged. The changes, all from the PR #13825 diff:

- **New `effective_config`:** `ConfigWrapper` gets a second slot, `effective_config`, built once in `__init__` by a new `_build_effective_config()`. `config_dict` is now documented as "as declared by the user… must never be mutated."
- **Derivation on a copy:** `_build_effective_config()` applies the `populate_by_name` and `validate_by_alias=False` rules to a copy (`{**config, ...}`). It also raises the "at least one of `validate_by_alias` or `validate_by_name`" `PydanticUserError`, now at construction time rather than during core-config building.
- **Readers switched over:** `core_config()` and `__getattr__` now read `effective_config`. The mutating block was removed from `core_config()`.
- **Copy on entry:** `prepare_config()` now calls `config.copy()` so the wrapper never mutates a user-provided mapping.
- **Deprecation warning moved:** the `schema_generator` deprecation warning moved into `check_deprecated()`, so it fires once per config.
- **Other files:**
  - `deprecated/decorator.py` builds the config dict before wrapping it, so its internal `extra='forbid'` no longer leaks into the user's config.
  - `validate_by_name` was removed from `_signature.py`, `_dataclasses.py` and `_model_construction.py`. Signature generation now reads only `validate_by_alias`.
- **Tests:** `test_derived_alias_config_not_inherited` reproduces the issue. `test_config_not_mutated` covers models, `TypeAdapter`, dataclasses and `validate_call`. `test_dynamic_default` now asserts the behaviour (name works, alias-only fails) instead of the contents of `model_config`.

**Behaviour change:** `Model.model_config` now shows only the declared keys, e.g. `{'populate_by_name': True}`, not the derived `validate_by_*` keys. The PR labels this a theoretical breaking change that needs a blog post entry.

**Not verified:** the patch for `tests/test_model_signature.py` was truncated in the output, and I didn't read the full text of the merged code, only the diff.