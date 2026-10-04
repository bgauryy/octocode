**Root cause.** `ConfigWrapper.core_config()` wrote derived settings straight into `self.config_dict`. That dict is the model's own `model_config`, or the mapping the caller passed in. Two derivations were involved:
- `populate_by_name` was turned into `validate_by_alias=True` plus `validate_by_name=<populate_by_name>`.
- `validate_by_alias=False` was turned into `validate_by_name=True`.

Both ran only when `validate_by_name` was `None`.

A subclass's config is merged over its parent's. So the parent's derived `validate_by_name=True` was inherited as if the user had declared it. The `is None` guard then stopped the subclass's own `populate_by_name=False` from deriving a new value. In the issue, `Child` bypassed its alias requirement while an identical `Standalone` enforced it. The same mutation also changed user-owned dicts passed to `TypeAdapter(config=...)` and `validate_call(config=...)`. The issue itself is #13786.

**Fix.** PR #13825 was merged on 2026-09-25 and closed the issue. The other linked PR, #13794, was closed without merging. The fix, in `pydantic/_internal/_config.py`, stops mutating the declared config and derives an effective copy instead:
- `ConfigWrapper` gains an `effective_config` slot beside `config_dict`. `config_dict` is documented as the declared config, which is merged on subclassing and "must never be mutated".
- A new `_build_effective_config()` builds a copy with `{**config, ...}` and applies the `populate_by_name` and `validate_by_alias=False` derivations. It also raises the "At least one of `validate_by_alias` or `validate_by_name`…" error.
- `__init__` sets `effective_config = _build_effective_config(self.config_dict)`. `__getattr__` and `core_config()` now read `effective_config`.
- `prepare_config()` now copies the user's dict, so the wrapper never mutates the caller's mapping.
- The `schema_generator` deprecation warning moved into `check_deprecated()`, so it is emitted once per config.

The PR made some related changes:
- It dropped the `validate_by_name` parameter from the signature generation code in `_signature.py`, `_model_construction.py` and `_dataclasses.py`.
- It updated `deprecated/decorator.py` to use `prepare_config` and set `extra='forbid'` on the copy.
- `test_dynamic_default` was changed to assert that the derived default is applied rather than visible on `model_config`, as the issue anticipated.
- It added `test_derived_alias_config_not_inherited` and `test_config_not_mutated`.

**Behaviour change.** `Model.model_config` now stays `{'populate_by_name': True}` instead of gaining `validate_by_alias` and `validate_by_name`. The PR flags this as needing a blog-post entry.

**Uncertainty.** The patch output was truncated for `tests/test_model_signature.py`, which I didn't read in full. I didn't read the merged source files, only the PR diff.