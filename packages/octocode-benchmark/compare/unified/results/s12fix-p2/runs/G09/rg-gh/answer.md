**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings back into `self.config_dict`. For a model, that dict is its own `model_config`. When `populate_by_name` is set and `validate_by_name` is unset, it stored `validate_by_alias=True` and `validate_by_name=<populate_by_name>` there. I read this from the issue text and PR #13787's description. I did not open the pre-fix source.

The derived values then looked like declared ones. A subclass's config is merged over its parent's, so `Child` inherited the parent's derived `validate_by_name=True`. The `is None` guard then refused to re-derive from the child's own `populate_by_name=False`. The result in the issue's example:
- `Standalone` correctly requires the alias `X`.
- `Child` silently accepts `x=1`.

The same mutation also changed user-owned mappings passed to `TypeAdapter(config=...)` and `validate_call(config=...)`.

**Fix:** PR #13825, "Do not mutate `model_config` attribute", merged as `5da36b5de4f44a572ca5c12104fd2f8669dfeaca`. Its description says "Fixes #13786". The issue was closed.
- The derived `validate_by_*` values are no longer stored on `model_config`. For `model_config = {'populate_by_name': True}`, it now stays `{'populate_by_name': True}` instead of gaining `validate_by_alias` and `validate_by_name`.
- The PR changes `pydantic/_internal/_config.py` (+49/−28). It also touches `_dataclasses.py`, `_model_construction.py`, `_signature.py`, `pydantic/config.py` and `pydantic/deprecated/decorator.py`, plus tests in `tests/test_config.py`, `tests/test_deprecated_validate_arguments.py` and `tests/test_model_signature.py`.
- The PR notes this as a theoretical breaking change that needs a blog post entry.

**Other PRs:** PR #13787 and PR #13794 were both closed without merging. They proposed deriving on a copy inside `core_config` (#13794 used `self.config_dict.copy()`). I didn't read the diffs of #13825, so I can't confirm whether it takes the same copy approach or a different one.

**Not verified:** I didn't open the #13825 diff, so I haven't checked how it changed `_config.py`. I also didn't check how it handled `test_dynamic_default`, which the issue says asserts the derived value is visible on `model_config`.