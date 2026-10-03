**Answer:** The helper is `merge_content(first_content, *contents)` at `libs/core/langchain_core/messages/base.py:366`. If it took exactly two contents, the two call sites that spread many contents into it would break. The other callers already pass two. The public export and the test would need updating.

**Affected: callers that spread many contents**
- `BaseMessageChunk.__add__`, list branch, `base.py:453`: `merge_content(self.content, *(o.content for o in other))`. This is where a chunk is added to a list of chunks.
- `add_ai_message_chunks` in `ai.py:665`: `merge_content(left.content, *(o.content for o in others))`. It is variadic, taking `left` plus `*others`.

Both would need a different approach, such as folding pairwise with `functools.reduce` or a loop. That would be a behaviour risk as well as a signature change. The current loop (`base.py:~380-400`) merges sequentially and mutates `merged` in place, for example `merged[-1] += content`. Pairwise folding should give the same result, but I did not verify that.

**Not affected: callers that already pass two**
- `base.py:442`, the single-chunk branch of `BaseMessageChunk.__add__`.
- `chat.py:43` and `chat.py:55`.
- `function.py:52`.
- `tool.py:191`.

**Other things touched**
- The public re-export: `messages/__init__.py:19` (import), `:123` (`__all__`) and `:139` (lazy-import map). Since the function is public, external callers passing 3 or more contents would also break. Nothing in `__init__.py` would need to change for the export itself.
- Tests: `tests/unit_tests/test_messages.py:1104-1109`, `test_merge_content`, calls `merge_content(first, *others)`. It would fail for any case where `others` has more than one item. `tests/unit_tests/messages/test_imports.py:47` only checks the export name.

**Uncertainty:** I searched only `libs/core` for the literal `merge_content`. I did not check other packages in the monorepo, which may also use it. I also did not read the test's parametrized cases to see whether any pass more than one extra content, or read `ai.py` beyond lines 655-670.