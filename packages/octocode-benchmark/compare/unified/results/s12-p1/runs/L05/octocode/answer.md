**Short answer:** Two call sites in langchain-core pass more than two contents and would break. The other call sites already pass exactly two. The function is `merge_content(first_content, *contents)` at `libs/core/langchain_core/messages/base.py:366-369`.

**Call sites that pass more than two contents (would break):**
- `libs/core/langchain_core/messages/base.py:453` is in `BaseMessageChunk.__add__`, in the branch where `other` is a list of chunks. It calls `merge_content(self.content, *(o.content for o in other))`, which is variadic. It would need a loop or a `functools.reduce` over the list.
- `libs/core/langchain_core/messages/ai.py:665` is in the AI-message-chunk merge helper that takes `left` and `others`. It calls `merge_content(left.content, *(o.content for o in others))`. It would need the same change.

**Call sites that already pass exactly two (unaffected):**
- `base.py:442`, `chat.py:43` and `chat.py:55`, `function.py:52`, and `tool.py:191`.

**Public API and exports:** `merge_content` is a public export. It appears in `messages/__init__.py` at line 19 (the type-checking import), line 123 (`__all__`), and line 139 (the lazy-import map). It is also listed in `tests/unit_tests/messages/test_imports.py:47`. Changing the signature changes the public API for external callers, but the export list itself would not change.

**Tests:** `tests/unit_tests/test_messages.py:1104-1110` (`test_merge_content`) calls `merge_content(first, *others)`. Its parametrized `others` values would break wherever they hold more than one item. They would need rewriting or chaining.

**Not verified:** I searched only `libs/core`. Other packages in the monorepo, such as partners, may also call `merge_content`. I did not check whether any `libs/core` docs or docstrings describe the variadic form.