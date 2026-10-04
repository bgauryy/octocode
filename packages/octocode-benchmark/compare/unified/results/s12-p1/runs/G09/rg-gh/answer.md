**Root cause.** `ConfigWrapper.core_config()` wrote derived values back into the mapping it was given. For a model, that mapping is the model's own `model_config`.

- The code handled the backwards-compatibility case `populate_by_name` → `validate_by_name`/`validate_by_alias`. It also set `validate_by_name=True` when `validate_by_alias=False`. Both wrote into `self.config_dict`.
- A subclass's config is merged over its parent's, so the parent's derived `validate_by_name=True` was inherited as if the user had declared it.
- The `config.get('validate_by_name') is None` guard then stopped the subclass's own `populate_by_name=False` from deriving again.
- In the issue's example, `Child(x=1)` bypassed the required alias, while an identical standalone model raised an error.
- The same write also changed caller-owned `ConfigDict`s passed to `TypeAdapter`, `validate_call` and `@dataclass`.

**Fix.** Issue #13786 was closed on 2026-09-25. The merged PR is #13825 by Viicos, merge commit `5da36b5de4f44a572ca5c12104fd2f8669dfeaca`. Its description says "Fixes #13786".

- `ConfigWrapper` now has two attributes (`pydantic/_internal/_config.py`, slots at about line 62). `config_dict` holds the declared config, is exposed as `model_config`, is used for inheritance merging, and is never mutated. `effective_config` holds the config with the deprecated-setting handling applied.
- `effective_config` is built in `__init__` by `_build_effective_config(self.config_dict)`.
- Both `__getattr__` and `core_config()` now read from `effective_config`.
- The derivation code was removed from `core_config()`. The diff I saw was truncated, so I did not see the body of `_build_effective_config`.
- After the fix, `Model.model_config` for `{'populate_by_name': True}` stays `{'populate_by_name': True}`. The PR flags this as a theoretical breaking change that needs a blog entry.

Two earlier PRs were closed without merging. #13787 derived on a copy inside `core_config`, and #13794 used `self.config_dict.copy()`.

**Uncertainty.** The PR #13825 patch output was cut off. I did not see its test changes or the rest of the `_config.py` diff. I also did not check whether the merge commit is on `main` or in a release.