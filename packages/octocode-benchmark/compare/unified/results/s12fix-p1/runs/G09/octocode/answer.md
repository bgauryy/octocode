**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings back into `self.config_dict`. That dict is the model's own `model_config`, or the caller's mapping for `TypeAdapter`, `validate_call` and dataclasses. This is from the issue text and the removed lines in the PR diff.

The writes came from these backwards-compatibility rules:
- If `populate_by_name` is set and `validate_by_name` is `None`, set `validate_by_alias=True` and `validate_by_name=populate_by_name`.
- If `validate_by_alias` is `False` and `validate_by_name` is `None`, set `validate_by_name=True`.

Two problems followed:
1. **Inheritance bug.** A subclass's config is merged over its parent's. The parent's derived `validate_by_name=True` was stored in `model_config`, so it was inherited as if the user had declared it. In the subclass, the `validate_by_name is None` guard then refused to re-derive from the subclass's own `populate_by_name=False`. In the issue's example, `Child` therefore accepted `x=1` and bypassed the alias requirement that `Standalone` enforces.
2. **Caller mutation.** The same write changed user-owned config mappings passed to `TypeAdapter(config=...)` and `validate_call(config=...)`.

**Fix:** PR #13825, merged 2026-09-25 (it closed the issue). A competing PR, #13794, was closed without merging. The changes:
- `ConfigWrapper` gets a second slot, `effective_config`.
  - `config_dict` now holds only what the user declared and must never be mutated. It is what `model_config` exposes and what inheritance merges.
  - `effective_config` is built once in `__init__` by a new `_build_effective_config()`.
- `_build_effective_config()` applies the `populate_by_name` and `validate_by_alias` derivations to a copy (`{**config, ...}`). It also raises the "At least one of `validate_by_alias` or `validate_by_name` must be set to True" error.
  - `__getattr__` and `core_config()` now read `effective_config`.
  - Because the error is now raised in `__init__`, it fires eagerly, for example with `defer_build=True`. A new test covers this.
- `prepare_config()` now copies the user's dict (`config.copy()`), so the wrapper owns its config.
- The `schema_generator` deprecation warning moved into `check_deprecated()`, so it fires once per config rather than on every core-config build.
- `deprecated/decorator.py` no longer sets `extra='forbid'` on the caller's dict. It calls `prepare_config` first and then builds the wrapper.
- `validate_by_name` was removed from the signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`. In `_signature.py`, a non-identifier alias now always falls back to `**kwargs`.
- Tests added: `test_derived_alias_config_not_inherited` (the issue's scenario), `test_config_not_mutated`, and a rewritten `test_dynamic_default`. That test now checks that the derived default is applied rather than that it appears on `model_config`, as the issue anticipated.

**Behaviour change:** `Model.model_config` is no longer mutated. `{'populate_by_name': True}` stays as declared and no longer gains `validate_by_alias` and `validate_by_name`. The PR flags this as a theoretical breaking change that needs a blog post entry.

**Uncertainty:** I did not read the final merged source files, only the PR diff. The last patch, `tests/test_model_signature.py`, was truncated. I did not look at why #13794 was closed.