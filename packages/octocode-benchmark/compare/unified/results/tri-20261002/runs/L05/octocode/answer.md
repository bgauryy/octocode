If `merge_content` took exactly two contents, the call sites that pass several contents at once would break. Those are `BaseMessageChunk.__add__` for a list of chunks and `add_ai_message_chunks`. The two-argument call sites would still work.

The helper is `merge_content(first_content, *contents)` at `libs/core/langchain_core/messages/base.py:366-369`. It loops over `contents` at line 383.

**Would break (variadic calls):**
- `libs/core/langchain_core/messages/base.py:453`: `BaseMessageChunk.__add__` has a branch for `other` being a list of chunks. It calls `merge_content(self.content, *(o.content for o in other))`, which passes N+1 contents. This would need a loop or `functools.reduce`.
- `libs/core/langchain_core/messages/ai.py:665`: `add_ai_message_chunks` calls `merge_content(left.content, *(o.content for o in others))`. It is the same problem, and it is the path `AIMessageChunk.__add__` uses to merge several chunks.
- `libs/core/tests/unit_tests/test_messages.py:1109`: `test_merge_content` calls `merge_content(first, *others)`. Its parametrized cases with more than two contents would need to change.

**Unaffected (already two arguments):**
- `base.py:442`
- `chat.py:43` and `chat.py:55`
- `function.py:52`
- `tool.py:191`

**Public API surface:**
- `merge_content` is exported from `langchain_core.messages`. It appears in `__init__.py:19`, in `__all__` at line 123, and in the lazy-import map at line 139. It is also listed in `tests/unit_tests/messages/test_imports.py:47`.
- Narrowing the signature is a breaking change for external callers that pass several contents. The import tests would still pass, because the name stays.

**Caveat:** I only searched `libs/core`. Other packages in the repo, such as partners, may also call `merge_content`, and I did not check them.