**Root cause.** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings back into `self.config_dict`. That dict is the model's own `model_config`, or the mapping the caller passed to `TypeAdapter`, `validate_call` and similar entry points. This was the code that did it:

```python
if (populate_by_name := config.get('populate_by_name')) is not None:
    if config.get('validate_by_name') is None:
        config['validate_by_alias'] = True
        config['validate_by_name'] = populate_by_name
```

A second rule did the same for `validate_by_alias=False`, setting `validate_by_name=True`.

- **Inheritance bug.** A subclass's config is merged over its parent's. The parent's derived `validate_by_name=True` was stored in `model_config`, so it was inherited as if the user had declared it. The `is None` guard then refused to derive again from the child's own `populate_by_name=False`.
- **Effect in the issue's example.** `Child` ended up with `validate_by_name=True` and accepted `x=1`, bypassing the alias requirement. `Standalone`, with the same declared config, correctly raised a `ValidationError`.
- **Side effect.** The same write also mutated caller-owned dicts passed to `TypeAdapter(config=...)` and `validate_call(config=...)`.

**Fix.** PR #13825, "Do not mutate `model_config` attribute", by Viicos. It was merged on 2026-09-25 and closed the issue. The author's earlier PR #13787 ("Derive alias config on a copy") was closed unmerged on 2026-09-17, the same day #13825 was opened. The changes below come from the #13825 diff:

- **Two config views.** `ConfigWrapper` now keeps `config_dict`, the user-declared config. It is exposed as `model_config`, used for merging, and never mutated. A new `effective_config` is built once in `__init__` by a new `_build_effective_config()`. That function derives `validate_by_alias` and `validate_by_name` on a copy (`{**config, ...}`), covering both the `populate_by_name` rule and the `validate_by_alias=False` rule. It also raises the "At least one of `validate_by_alias` or `validate_by_name`..." `PydanticUserError`.
- **Readers use the effective view.** `core_config()` and the wrapper's `__getattr__` now read `effective_config`.
- **Copy on entry.** `prepare_config()` now calls `config.copy()` so the wrapper owns its dict and cannot mutate the caller's.
- **Related cleanups.**
  - The `schema_generator` deprecation warning moved into `check_deprecated`, so it is emitted once per config.
  - The deprecated `validate_arguments` decorator no longer mutates the passed config when it sets `extra='forbid'`.
  - The `validate_by_name` argument was dropped from the signature generation in `_signature.py`, `_model_construction.py` and `_dataclasses.py`.
- **Tests.** `test_derived_alias_config_not_inherited` reproduces the issue. `test_config_not_mutated` covers models, `TypeAdapter`, dataclasses and `validate_call`. `test_dynamic_default` was rewritten to assert the derived default is applied instead of checking `model_config`.

**Behavior change.** `Model.model_config` now returns only what was declared, for example `{'populate_by_name': True}` and not the derived keys. The PR flags this as a theoretical breaking change that needs a blogpost entry.

**Uncertainty.** I read the issue, the PR body and the PR diff. I did not read the merged files at the merge commit, and I did not read the PR's review comments.