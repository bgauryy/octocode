**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` turned the deprecated `populate_by_name` into `validate_by_alias` and `validate_by_name` by writing into `self.config_dict`. That dict is the model's own `model_config` (issue #13786 quotes the code).

- **Inheritance bug:** The derived `validate_by_name=True` was stored next to the keys the user declared. A subclass's config is merged over the parent's, so the subclass inherited the derived value. The `if config.get('validate_by_name') is None` guard then refused to derive again from the subclass's own `populate_by_name=False`.
- **Example from the issue:** `Parent` sets `populate_by_name=True`. `Child(Parent)` sets `populate_by_name=False` and declares `x: int = Field(alias='X')`. `Child(x=1)` wrongly succeeds, while an otherwise identical standalone model raises a missing-field error.
- **Caller mutation:** With `TypeAdapter(config=...)` and `validate_call(config=...)`, the same write modified the caller's own `ConfigDict`.

**Fix:** PR #13825, "Do not mutate `model_config` attribute", merged 2026-09-25 as commit `5da36b5de4f44a572ca5c12104fd2f8669dfeaca`. It says "Fixes #13786".

- It touches `_config.py`, `_dataclasses.py`, `_model_construction.py`, `_signature.py`, `config.py`, `deprecated/decorator.py` and related tests.
- After the change, `Model.model_config` for `{'populate_by_name': True}` stays exactly that. Before, it also held `validate_by_alias` and `validate_by_name`.
- The PR notes this as a theoretical breaking change that needs a blog post entry.

Two earlier PRs proposed a narrower fix: #13787 and #13794 (from a different contributor). Both were closed without merging. They copied the config inside `core_config()` (`config = self.config_dict.copy()`) so the original dict stayed clean. #13787 also rewrote `tests/test_config.py::test_dynamic_default`, which had asserted that the derived values appear on `model_config`.

**Uncertainty:** I read the PR metadata and descriptions, not the #13825 diff. I don't know exactly how it stops the mutation across its nine files. It may not match the copy-based approach in #13787 and #13794. I also didn't check the changes to the tests.