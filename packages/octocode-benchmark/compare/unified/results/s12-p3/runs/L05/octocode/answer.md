**Short answer:** `merge_content` is defined at `libs/core/langchain_core/messages/base.py:366` as `merge_content(first_content, *contents)`. Limiting it to exactly two contents would break the callers that pass more than two. Those are the multi-chunk merge paths in `base.py` and `ai.py`. The rest only need their call style kept.

**Callers that pass exactly two contents.** These already fit a two-argument signature:
- `base.py:442`, in `BaseMessageChunk.__add__` for a single chunk: `merge_content(self.content, other.content)`.
- `chat.py:43` and `chat.py:55`, in the chat chunk's `__add__`.
- `function.py:52`, in the function chunk's `__add__`.
- `tool.py:191`, in the tool chunk's `__add__`.

**Callers that pass a variable number of contents.** These would break:
- `base.py:453`, in `BaseMessageChunk.__add__` when `other` is a list of chunks: `merge_content(self.content, *(o.content for o in other))`.
- `ai.py:665`, in the AI chunk merge helper that takes `left` and `others`: `merge_content(left.content, *(o.content for o in others))`. Its output is also used for the AI chunk's merged `additional_kwargs`, which is built right after it. This is the main place affected. It would have to merge pairwise, for example with `functools.reduce`, or the helper would have to change.

**Public API and tests:**
- `merge_content` is publicly exported. It appears in `messages/__init__.py` at line 19 (the import), line 123 (`__all__`) and line 139 (the lazy-import map). `tests/unit_tests/messages/test_imports.py:47` also lists it.
- External callers using one content or three or more would break.
- `tests/unit_tests/test_messages.py:1109` calls `merge_content(first, *others)` with a parametrized `others`. Any case that doesn't give exactly one other content would fail.

**Uncertainty:** I searched only `libs/core` for `merge_content`. I did not read the function body, so I didn't check how it treats the extra contents internally. I also did not check other packages in the repo, such as partners, for external callers.