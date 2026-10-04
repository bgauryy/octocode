**Root cause:** `ConfigWrapper.core_config()` wrote derived alias settings back into the model's own config dict (`self.config_dict`). That dict is the same object exposed as `model_config`. Pydantic merges a subclass's config over its parent's, so a derived value in `model_config` was treated as if the user had declared it.

The issue (#13786) gives this example. `Parent` declares `populate_by_name=True`, and `core_config()` then added `validate_by_alias=True` and `validate_by_name=True` to `Parent.model_config`. `Child` declares `populate_by_name=False`. It inherits the derived `validate_by_name=True`, so the `is None` guard (`if config.get('validate_by_name') is None`) skips re-deriving from the child's own setting. As a result, `Child(x=1)` is accepted even though the field has `alias='X'`, while an identical `Standalone` model raises `ValidationError`.

The same write also mutated user-owned dicts passed to `TypeAdapter(config=...)` and `validate_call(config=...)`. The report says it reproduced on 2.13.5 and on main (2.14.0b1).

**Fix:** PR #13825, "Do not mutate `model_config` attribute", by Viicos. It is merged as `5da36b5de4f44a572ca5c12104fd2f8669dfeaca` and its body says "Fixes #13786". The issue is closed. I did not confirm the merge commit is in a specific release. Two other PRs that referenced the issue, #13787 and #13794, were closed without merging.

The changes in the PR, all in `pydantic/_internal/_config.py` unless noted:
- **`effective_config`:** `ConfigWrapper` gets a new slot, `effective_config`, built by a new `_build_effective_config()`. It derives `validate_by_alias` and `validate_by_name` from `populate_by_name` or `validate_by_alias=False` on a copy of the declared config. It also raises the "at least one of `validate_by_alias` or `validate_by_name`" error.
- **Readers use the copy:** `__getattr__` and `core_config()` read from `effective_config`. `config_dict` stays exactly as declared and is never mutated.
- **Copy on input:** `prepare_config()` now copies a dict config, so the wrapper never mutates the caller's mapping.
- **Deprecation warning:** The `schema_generator` deprecation warning moved into `check_deprecated()`, so it is emitted once per config rather than on each core config build.
- **Signature cleanup:** The `validate_by_name` argument was removed from the signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`.
- **`validate_arguments` fix:** `pydantic/deprecated/decorator.py` now sets `extra='forbid'` on a prepared copy, not on the caller's dict.
- **Intentional behaviour change:** `Model.model_config` no longer shows the derived keys. For `{'populate_by_name': True}` it now stays `{'populate_by_name': True}`. The PR notes this as a theoretical breaking change that needs a blogpost entry.

**Tests** (`tests/test_config.py`): `test_dynamic_default` now checks that the derived default is applied rather than where it is stored. This is the change the issue anticipated. The PR also adds `test_derived_alias_config_not_inherited`, which reproduces the issue, and `test_config_not_mutated`.

**Uncertainty:** I read the PR diff and the issue text, not a checkout of the repo. I have not seen how `effective_config` behaves at runtime, and I did not look at later follow-up commits.