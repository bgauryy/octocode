**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings into `self.config_dict`. That dict is the model's own `model_config`, or the mapping the caller passed in. The issue (#13786) describes it this way:

- **What it wrote:** `populate_by_name` became `validate_by_alias=True` and `validate_by_name=<populate_by_name>`. The code also set `validate_by_name=True` when `validate_by_alias=False`.
- **Why subclasses broke:** a subclass's config is merged over its parent's. The parent's derived `validate_by_name=True` was therefore inherited as if the user had declared it. The `config.get('validate_by_name') is None` guard then stopped the child's own `populate_by_name=False` from being derived. In the issue's example, `Child` silently skipped the alias requirement that `Standalone` enforced.
- **Side effect:** the same write also changed user-owned mappings passed to `TypeAdapter(config=...)` and `validate_call(config=...)`.

**Fix:** PR #13825, merged 2026-09-25 (it closed the issue; PR #13794 was closed without merging). The changes are:

- **Derived settings:** a new `_build_effective_config()` computes them on a copy, `{**config, ...}`. It also raises the "at least one of `validate_by_alias`/`validate_by_name`" error.
- **Two config views on `ConfigWrapper`:**
  - `config_dict` is the declared config. It is never mutated, and it is what `model_config` and subclass merging use.
  - A new `effective_config` is built in `__init__`. Both `__getattr__` and `core_config()` now read from it.
- **Copying:** `prepare_config()` now copies the user's dict, so a `ConfigWrapper` never mutates the caller's mapping.
- **Related edits:**
  - The `schema_generator` deprecation warning moved into `check_deprecated`, so it is emitted once per config.
  - The `validate_by_name` argument was dropped from the signature generation functions.
  - `deprecated/decorator.py` now prepares the config dict before setting `extra='forbid'`, so it no longer mutates the caller's config.
- **Tests:** `tests/test_config.py` gains `test_derived_alias_config_not_inherited`, which references #13786, and `test_config_not_mutated`. `test_dynamic_default` no longer asserts that the derived values appear on `model_config`; it now checks that they are applied.

**Behavior change:** `Model.model_config` now returns only what the user declared, e.g. `{'populate_by_name': True}`. Before, it also contained `validate_by_alias` and `validate_by_name`. The PR flags this as needing a blog entry.

**Caveat:** the patch output was truncated at the end, so I did not read the `tests/test_model_signature.py` change in full. That does not affect the root-cause or fix summary.