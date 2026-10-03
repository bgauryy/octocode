If `merge_content` took exactly two contents, only two call sites in langchain-core would break. Those are the multi-chunk paths that pass several contents at once. The two-argument call sites would keep working unchanged.

**Definition:** `merge_content(first_content, *contents)` is at `libs/core/langchain_core/messages/base.py:366`. It's exported publicly through `messages/__init__.py:123` and `:139`.

**Would break (spread more than one other content):**
- `libs/core/langchain_core/messages/base.py:453`: `BaseMessageChunk.__add__` calls `merge_content(self.content, *(o.content for o in other))` when `other` is a list of chunks. It would need a pairwise fold, for example `functools.reduce`, or a loop.
- `libs/core/langchain_core/messages/ai.py:665`: `add_ai_message_chunks(left, *others)` calls `merge_content(left.content, *(o.content for o in others))`. It has the same problem and would need the same fold.

**Unaffected (already pass exactly two):**
- `base.py:442`
- `chat.py:43` and `chat.py:55`
- `function.py:52`
- `tool.py:191`

Each of these calls `merge_content(self.content, other.content)`.

**Tests and exports:**
- `libs/core/tests/unit_tests/test_messages.py:1104-1109` calls `merge_content(first, *others)`. `others` is always a one-element list in the parametrized cases I saw (`test_messages.py:1085-1098`). That test would only need its call changed to something like `merge_content(first, others[0])`.
- `libs/core/tests/unit_tests/messages/test_imports.py:47` only checks the export name, so it wouldn't change.
- The signature change is a breaking change for any external caller that passes three or more contents. I only searched inside `libs/core`, so I can't say whether other packages in the repo call it.

**Behaviour to check:** I didn't read the body of `merge_content` past its docstring. A pairwise fold might not give the same result as the current single-pass merge for mixed str/list inputs. The existing tests only cover two-content cases, so they wouldn't catch a difference.