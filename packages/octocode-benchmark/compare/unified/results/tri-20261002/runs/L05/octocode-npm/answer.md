**Direct answer:** `merge_content(first_content, *contents)` is defined at `libs/core/langchain_core/messages/base.py:366`. Making it take exactly two contents would break the two variadic call sites (`base.py:453` and `ai.py:665`) and the variadic test (`tests/unit_tests/test_messages.py:1109`). The remaining call sites already pass two arguments. The public export would be unaffected, apart from the signature change.

All paths below are under `libs/core/`. These come from a lexical search of langchain-core only, and I did not run LSP reference checks.

**Call sites that already pass two contents (unaffected):**
- `langchain_core/messages/base.py:442`: `BaseMessageChunk.__add__`, called as `merge_content(self.content, other.content)`.
- `langchain_core/messages/chat.py:43` and `:55`: both are two-argument calls.
- `langchain_core/messages/function.py:52`: two-argument call.
- `langchain_core/messages/tool.py:191`: two-argument call.

**Call sites that would break (they unpack a variable number of contents):**
- `langchain_core/messages/base.py:453`: the list branch of `BaseMessageChunk.__add__`. It calls `merge_content(self.content, *(o.content for o in other))`, so `self + [chunk1, chunk2, …]` would no longer work. The same branch also passes `*(o.additional_kwargs …)` to `merge_dicts`, which is a separate helper.
- `langchain_core/messages/ai.py:665`: the `add_ai_message_chunks` helper. It calls `merge_content(left.content, *(o.content for o in others))`, so merging several `AIMessageChunk`s at once would break. I did not check what depends on that helper.
  - Both sites could be rewritten to fold pairwise, for example with `functools.reduce`. That changes how the merge is done, though.

**Public API and tests:**
- `merge_content` is exported from `langchain_core.messages` at `langchain_core/messages/__init__.py:19`, listed in `__all__` at `:123`, and in the lazy-import map at `:139`. The export stays, but the public signature changes, so any external caller passing more than two contents would break.
- `tests/unit_tests/test_messages.py:1104-1109`: `test_merge_content` calls `merge_content(first, *others)`. If any parametrized case has more than one item in `others`, it would fail. I did not read the cases.
- `tests/unit_tests/messages/test_imports.py:47`: only checks that the name is exported, so it is unaffected.

**Uncertainty:** I searched only `libs/core`. Other packages in the repo, such as partners or `langchain` itself, may also call `merge_content`, and I did not check.