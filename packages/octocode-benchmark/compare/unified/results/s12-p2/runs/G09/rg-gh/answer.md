**Root cause:** `ConfigWrapper.core_config()` in `pydantic/_internal/_config.py` wrote derived settings into `self.config_dict`, which is the mapping it was given. For a model, that mapping is `model_config`.

- **How the derivation worked:** when `populate_by_name` was set and `validate_by_name` wasn't, it set `validate_by_alias=True` and `validate_by_name=<populate_by_name>`.
- **Why it broke inheritance:** a subclass's config is merged over its parent's. The parent's derived `validate_by_name=True` therefore reached the child and looked like a declared value. The `is None` guard then refused to derive again from the child's own `populate_by_name=False`.
- **Example from the issue:** a `Child` that declared `populate_by_name=False` still accepted `x=1` when `Standalone` (with the same config) required the alias `X`.
- **Side effect:** the same write also changed caller-owned `ConfigDict`s passed to `TypeAdapter(config=...)`, `validate_call(config=...)` and `@dataclass(config=...)`.

Source: the issue body and the description of PR #13787.

**Fix:** PR #13825, "Do not mutate `model_config` attribute", is the only one of the three linked PRs marked merged. Its description says "Fixes #13786".
- **What it changed:** it stops Pydantic from mutating `model_config`. It touches `_config.py` (+49/−28), `_dataclasses.py`, `_model_construction.py`, `_signature.py`, `config.py`, `deprecated/decorator.py` and the tests.
- **Visible behavior change:** `Model.model_config` for `{'populate_by_name': True}` is now just `{'populate_by_name': True}`. Before, it also contained `validate_by_alias` and `validate_by_name`. The PR notes this as a theoretical breaking change that needs a blog-post entry.

**Not merged:** PR #13787 and PR #13794 were both closed without merging. #13787 derived the values on a copy inside `core_config`. #13794 was the one-line `self.config_dict.copy()` change.

**Uncertainty:** I didn't read the merged diff of #13825 line by line. I also didn't check the merge commit or confirm which release contains it. The details of the fix come from that PR's description and file list.