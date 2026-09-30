**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings back into `self.config_dict`. That dict is the model's own `model_config`, or the caller's mapping for `TypeAdapter(config=...)` and `validate_call(config=...)`. The issue quotes the writes:

- When `populate_by_name` is set and `validate_by_name` is `None`, it set `validate_by_alias = True` and `validate_by_name = populate_by_name`.
- When `validate_by_alias` is `False` and `validate_by_name` is unset, it set `validate_by_name = True`.

Because those derived values were stored beside the declared ones, they took part in the config merge on subclassing.

1. `Parent` declared `populate_by_name=True`, so `Parent.model_config` gained `validate_by_name=True`.
2. `Child` declared `populate_by_name=False` and inherited that derived `validate_by_name=True`.
3. The `validate_by_name is None` guard then refused to derive again from `Child`'s own `populate_by_name=False`.

So `Child(x=1)` silently skipped the alias requirement that an identical standalone model enforces. The same write also mutated user-owned config dicts passed to `TypeAdapter` and `validate_call`.

**Fix:** PR #13825, "Do not mutate `model_config` attribute" by Viicos, merged 2026-09-25. It is linked to the issue as a fix, and the issue closed at the same time. I read the diff but not the tests.

- **Two configs on the wrapper:** `ConfigWrapper` now has `config_dict`, the config as declared, and `effective_config`, the config in effect. The docstring says `config_dict` is what `model_config` exposes and what gets merged with bases, so it must never be mutated.
- **Derivation on a copy:** a new `_build_effective_config()` builds `effective_config` in `__init__`. It returns a copied dict, `{**config, ...}`, with the `validate_by_alias` and `validate_by_name` values derived. It also raises the "at least one of `validate_by_alias` or `validate_by_name`" `PydanticUserError`.
- **Readers switched over:** `core_config()` and `__getattr__` now read `effective_config`. The old in-place patching and the error check were removed from `core_config()`.
- **Caller dicts protected:** `prepare_config()` now copies a caller-supplied dict, so the wrapper never mutates the user's mapping.
- **Related cleanups:**
  - The `schema_generator` deprecation warning moved to `check_deprecated()`.
  - `validate_by_name` was dropped from the signature-generation code (`_signature.py`, `_model_construction.py`, `_dataclasses.py`).
  - `deprecated/decorator.py` was adjusted to build and copy the config dict itself.

The PR body flags one behaviour change. `Model.model_config` now stays `{'populate_by_name': True}` instead of also showing the derived keys. That is the trade-off the issue raised about `test_dynamic_default`.

**Not fixed by other PRs:** #13787 and #13794 were closed without merging.

**Uncertainty:** I did not read the tests changed in #13825.