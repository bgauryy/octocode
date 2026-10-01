**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` changed the user's own config dict in place. That dict is the same object exposed as `model_config`. When `populate_by_name` was set and `validate_by_name` was `None`, it wrote `validate_by_alias=True` and `validate_by_name=<populate_by_name>` into it.

Those derived keys were then stored beside the declared ones, so they were indistinguishable from user settings. A subclass's config is merged over its parent's, so:

- A parent's derived `validate_by_name=True` was inherited by the child.
- The `validate_by_name is None` guard then refused to derive again from the child's explicit `populate_by_name=False`.
- The result was that `Child` silently accepted `x=1` and skipped the alias requirement that `Standalone` enforced.

The same write also changed mappings the caller passed to `TypeAdapter(config=...)` and `validate_call(config=...)`. (Source: the issue body for #13786; the removed code is visible in the PR #13825 diff.)

**Fix:** PR #13825, "Do not mutate `model_config` attribute", by Viicos. It merged on 2026-09-25 and closed the issue. Two earlier PRs, #13787 and #13794, were closed without merging. The issue author's proposal was to derive on a copy inside `core_config`. The merged PR instead changed `ConfigWrapper` in `_config.py`:

- **Declared vs. effective config:** `config_dict` now holds only what the user declared and is never mutated. A new `effective_config` slot holds the derived result.
- **Derivation moved to a helper:** a new `_build_effective_config()` builds `effective_config` from `config_dict` when the wrapper is created. It returns a copy (`{**config, ...}`) when it derives `validate_by_alias`/`validate_by_name` from `populate_by_name` or from `validate_by_alias=False`. It also raises the "at least one of validate_by_alias or validate_by_name" error, so that error now fires when the wrapper is created instead of when the core schema is built.
- **Readers switched over:** `__getattr__` and `core_config()` read `effective_config`.
- **Caller's dict protected:** `prepare_config` now copies dicts passed in, so the wrapper owns its config.
- **Deprecation warning moved:** the `schema_generator` warning moved into `check_deprecated`.
- **Signature cleanup:** the `validate_by_name` argument was removed from the signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`.
- **Deprecated decorator:** `deprecated/decorator.py` was changed so it no longer writes `extra='forbid'` into the caller's config.

**Behaviour change:** `Model.model_config` no longer shows the derived keys. For `{'populate_by_name': True}` it is now just that, where before it also had `validate_by_alias` and `validate_by_name`. The PR is labelled `needs-blogpost-entry` for this reason.

**Tests:** `test_dynamic_default` now checks that the derived default is applied instead of checking what `model_config` contains. `test_derived_alias_config_not_inherited` (in `tests/test_config.py`) covers this issue. `test_config_not_mutated` covers `TypeAdapter`, dataclass and `validate_call`.

**Uncertainty:** I read the PR diff but did not open the merged source on the default branch. I saw only part of the `tests/test_model_signature.py` patch.