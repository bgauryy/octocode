**Root cause.** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings straight into `self.config_dict`. That dict is the model's own `model_config`, or the mapping the caller passed in. The issue describes the writes like this:

- `populate_by_name` became `validate_by_alias=True` and `validate_by_name=<value>`.
- `validate_by_alias=False` became `validate_by_name=True`.

Both writes were guarded by `validate_by_name is None`.

Subclass config is merged over the parent's, so a derived `validate_by_name=True` stored on `Parent.model_config` was inherited by `Child`. When `Child` declared `populate_by_name=False`, the `is None` guard blocked a fresh derivation, because the key was already set. The result was that `Child` kept validating by name while `Standalone` didn't. The alias requirement was silently bypassed.

The same write also mutated user-owned mappings passed to `TypeAdapter(config=...)` and `validate_call(config=...)`. The fix PR's tests also cover `pydantic_dataclass` configs and the deprecated `validate_arguments` decorator.

**Fix.** PR #13825, merged 2026-09-25, closed the issue. PR #13794 was also linked to the issue but was closed unmerged. The merged change did the following:

- **Separate declared from effective config.** `ConfigWrapper` now has a second slot, `effective_config`. `config_dict` holds only what the user declared. It is the mapping exposed as `model_config`, it is the one merged with the bases, and it must never be mutated. `effective_config` is built once in `__init__` by a new `_build_effective_config()`, which applies the derivations on a copy (`{**config, ...}`).
- **Read from the effective config.** `__getattr__` and `core_config()` now read `effective_config`.
- **Move the checks.** The "at least one of `validate_by_alias` / `validate_by_name`" `PydanticUserError` moved into `_build_effective_config()`, so it is raised eagerly. The `schema_generator` deprecation warning moved to `check_deprecated()`, so it fires once per config.
- **Copy user input.** `prepare_config()` now copies the user's dict, so the wrapper owns its config.
- **Remove `validate_by_name` from signature generation.** The `validate_by_name` parameter was dropped from signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`.
- **Deprecated decorator.** `deprecated/decorator.py` now builds its config via `prepare_config` instead of mutating `config_dict['extra']`.

**Behavior change.** `Model.model_config` no longer shows derived keys. With `populate_by_name=True` it is now `{'populate_by_name': True}`. The old result had `validate_by_alias` and `validate_by_name` added. The PR flags this as a theoretical breaking change that needs a blogpost entry.

**Tests.** `test_dynamic_default` now asserts the behavior rather than the dict contents, as the issue author anticipated. New tests are `test_derived_alias_config_not_inherited` (the issue's repro) and `test_config_not_mutated`.

**Uncertainty.** The patch output was truncated, so I didn't see the full `tests/test_model_signature.py` change. I didn't open the merged source at the merge commit. Everything above comes from the PR diff.