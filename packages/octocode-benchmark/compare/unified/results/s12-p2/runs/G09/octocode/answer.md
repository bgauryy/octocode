**Root cause:** `ConfigWrapper.core_config()` wrote derived alias settings into `self.config_dict`. That dict is the model's own `model_config`, or the user's mapping for `TypeAdapter`, `validate_call` and similar entry points. The code (quoted in the issue and removed in the PR's `pydantic/_internal/_config.py` diff) set `validate_by_alias=True` and `validate_by_name=populate_by_name` whenever `validate_by_name` was `None`.

Subclass config is merged over the parent's, so a parent's derived `validate_by_name=True` was inherited like a declared value. The `is None` guard then stopped the child's own `populate_by_name=False` from being derived again. So `Child` skipped the alias requirement that `Standalone` enforced. The same write also changed user-owned dicts passed to `TypeAdapter(config=...)` and `validate_call(config=...)` (issue #13786).

**Fix:** PR #13825, merged 2026-09-25. It closed the issue. A competing PR, #13794, was closed without merging. The PR stops mutating the declared config, in `pydantic/_internal/_config.py`:
- **`effective_config`:** `ConfigWrapper` gets a new slot, `effective_config`. `config_dict` stays as the declared config, is what gets merged on subclassing, and must never be mutated.
- **`_build_effective_config()`:** a new helper builds the effective config as a copy (`{**config, ...}`). It derives `validate_by_alias` and `validate_by_name` from `populate_by_name`, or sets `validate_by_name` when `validate_by_alias` is `False`. It also raises the "at least one of validate_by_alias/validate_by_name" `PydanticUserError`, so that error is now raised eagerly.
- **Readers:** `__getattr__` and `core_config()` now read `effective_config`.
- **`prepare_config`:** it now copies user-supplied dicts, so a user's mapping is never mutated.
- **Deprecation warning:** the `schema_generator` deprecation warning moved into `check_deprecated`, so it fires once per config.
- **Related changes:** `validate_by_name` was removed from the signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`. `deprecated/decorator.py` was adjusted to avoid mutating the config.

**Behaviour change:** `Model.model_config` now shows only what the user declared, for example `{'populate_by_name': True}`. The PR body flags this as a theoretical breaking change that needs a blogpost entry. As the issue anticipated, `test_dynamic_default` was changed to assert that the derived default is applied rather than where it is stored.

**Tests added** in `tests/test_config.py`: `test_derived_alias_config_not_inherited`, which references the issue, and `test_config_not_mutated`.

**Uncertainty:** the PR patch output was truncated, so I didn't read the last test file, `tests/test_model_signature.py`, in full.