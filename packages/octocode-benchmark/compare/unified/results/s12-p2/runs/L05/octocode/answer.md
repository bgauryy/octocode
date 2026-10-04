Making `merge_content` take exactly two contents would break two multi-argument call sites in langchain-core, plus the test. It would also change the public export. The five single-pair call sites would be unaffected.

Today the signature is `merge_content(first_content, *contents)` (`libs/core/langchain_core/messages/base.py:366-369`).

**Would break (more than two contents):**
- `BaseMessageChunk.__add__` has a branch for adding a list of chunks. It calls `merge_content(self.content, *(o.content for o in other))` at `base.py:453`. A list `other` with more than one element would no longer fit. That branch would need a fold or loop over `merge_content`, and the sibling `merge_dicts` calls on the next lines (`base.py:455`, `base.py:458`) would no longer match the content handling.
- `add_ai_message_chunks(left, *others)` calls `merge_content(left.content, *(o.content for o in others))` at `ai.py:665`. It also takes a variable number of chunks, so it would need to reduce pairwise.
- The unit test `test_merge_content` calls `merge_content(first, *others)` at `tests/unit_tests/test_messages.py:1109`. Its parametrized `others` is a list, so it would need rewriting.

**Unaffected (already two arguments):**
- `merge_content(self.content, other.content)` in `base.py:442`
- `chat.py:43` and `chat.py:55`
- `function.py:52`
- `tool.py:191`

**Public surface:**
- `merge_content` is re-exported from `langchain_core/messages/__init__.py` (import at line 19, `__all__` at line 123, lazy-import map at line 139).
- `tests/unit_tests/messages/test_imports.py:47` checks that name, so the export itself stays intact. Only its signature changes.
- Any external caller passing three or more contents would break.

**Uncertainty:**
- I only searched `libs/core`. Other packages in the repo may call `merge_content` too, and I didn't check them.
- I didn't read the function body (`base.py:373-435`). That means I didn't check whether it loops over `contents` in a way that a two-argument version would simplify.