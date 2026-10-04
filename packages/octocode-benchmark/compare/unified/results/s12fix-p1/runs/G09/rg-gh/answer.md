**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote its derived settings back into `self.config_dict`. That is the same mapping it was given, which for a model is `model_config`. When `populate_by_name` was set, it filled in `validate_by_alias=True` and `validate_by_name=<populate_by_name>`, guarded by `if config.get('validate_by_name') is None`. (The issue body quotes this code. I didn't open the source file itself.)

This broke inheritance in two ways:
- **Inheritance:** A subclass's config is merged over its parent's, so a subclass inherited the parent's derived `validate_by_name=True` as though the user had declared it. The `is None` guard then refused to derive again from the subclass's own `populate_by_name=False`.
- **Caller-owned configs:** The same write also mutated `ConfigDict`s passed directly to `TypeAdapter(config=...)` and `validate_call(config=...)`.

In the issue's example, `Child(x=1)` was accepted, while an identical `Standalone(x=1)` raised a missing-alias error.

**Fix:** The issue was closed by PR #13825, "Do not mutate `model_config` attribute". It is merged, with merge commit `5da36b5de4f44a572ca5c12104fd2f8669dfeaca`. I haven't read its diff; the following comes from its description and file list.
- **Behaviour change:** Pydantic no longer mutates `model_config`. `Model.model_config` stays `{'populate_by_name': True}` instead of gaining `validate_by_alias` and `validate_by_name`. The PR calls this a theoretical breaking change that needs a blog post entry.
- **Files touched:** `_config.py` (+49/−28), `_dataclasses.py`, `_model_construction.py`, `_signature.py`, `config.py`, `deprecated/decorator.py`, plus tests in `test_config.py`, `test_deprecated_validate_arguments.py` and `test_model_signature.py`.

Two other PRs proposed a narrower fix and were closed without merging:
- **#13787:** Derives the values on a copy inside `core_config`. It also rewrote `test_dynamic_default`, which asserted the derived value was visible on `model_config`.
- **#13794:** A one-line `self.config_dict.copy()` change.

**Uncertainty:** I haven't confirmed how #13825 stops the mutation internally. The file list suggests the derivation was reworked across several modules, but I didn't read the diff.