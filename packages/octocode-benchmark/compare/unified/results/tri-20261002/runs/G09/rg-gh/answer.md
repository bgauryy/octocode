**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings into `self.config_dict`. For a model, that dict is the model's own `model_config`, so pydantic's derived values were stored next to the ones the user declared.

- **Writes:** `populate_by_name=True` was turned into `validate_by_alias=True` and `validate_by_name=True`, and `validate_by_alias=False` was turned into `validate_by_name=True`. These writes are quoted in the issue body and in the removed lines of PR #13825.
- **Inheritance effect:** a subclass's config is merged over its parent's, so the parent's derived `validate_by_name=True` reached the child. The `is None` guard then refused to derive again from the child's own `populate_by_name=False`.
- **Result (the issue's example):** `Child` and `Standalone` declare the same config and field, but `Standalone(x=1)` raises a missing-alias error while `Child(x=1)` succeeds.
- **Other entry points:** the same write also mutated user-owned dicts passed to `TypeAdapter(config=...)` and `validate_call(config=...)`.

**Fix:** PR #13825, "Do not mutate `model_config` attribute", merged 2026-09-25 as `5da36b5de4f44a572ca5c12104fd2f8669dfeaca`. Its description says "Fixes #13786".

- **Two configs:** `ConfigWrapper` now has two mappings.
  - `config_dict` is the config as declared. It is exposed as `model_config`, used for merging with bases, and never mutated.
  - `effective_config` is built once in `__init__` by the new `_build_effective_config()`. It derives the deprecated settings on a copy (`{**config, ...}`) and raises the "at least one of `validate_by_alias` or `validate_by_name`" error there.
- **Consumers:** `__getattr__` and `core_config()` now read `effective_config`.
- **Copy on entry:** `prepare_config()` copies a user-supplied dict, so the wrapper never mutates the caller's mapping.
- **Related cleanups in the same PR:**
  - The `schema_generator` deprecation warning moved into `check_deprecated()`.
  - `validate_by_name` was dropped from signature generation.
  - The deprecated `validate_arguments` decorator no longer writes `extra='forbid'` into the user's config.
- **Behaviour change:** `Model.model_config` for `populate_by_name=True` is now just `{'populate_by_name': True}`. The PR description flags this as a theoretical breaking change that needs a blog post entry.

**Other PRs:** #13787 and #13794 (a simple `.copy()` in `core_config()`) were also opened for this issue. Both are closed and unmerged.

**Uncertainty:** I did not check whether `tests/test_config.py::test_dynamic_default` was changed. The issue says it asserts the derived value is visible on `model_config`, and my view of the PR's `tests/test_config.py` diff was truncated.