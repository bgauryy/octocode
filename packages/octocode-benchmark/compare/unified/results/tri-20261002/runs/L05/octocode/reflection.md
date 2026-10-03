**1. Helped:** My first call, `localSearch` for `merge_content` under `libs/core`, found every caller with line numbers, including the variadic `*(o.content for o in ...)` forms at `base.py:453` and `ai.py:665`. The second call, `localFetch` of `base.py:364-460`, gave the signature, the loop and the `__add__` context. Two calls were enough.

**2. Did not help / gaps:** The search was fine, but I stopped too early. I never read `ai.py:665`, `test_messages.py:1104-1109`, or the `chat.py`, `function.py` and `tool.py` call sites. I judged those from the grep lines alone. Two statements in my answer therefore went beyond what I saw:
- That `add_ai_message_chunks` is the path `AIMessageChunk.__add__` uses.
- That the parametrized test cases include more than two contents.

I also searched only `libs/core`, which I did say in the answer.

**3. Next time:** I would read `ai.py` around line 665 and the `test_merge_content` parameters before making those claims. I would also run one `localSearch` over the whole repo to catch callers in other packages.

**4. Confidence:** High that the `base.py:453` and `ai.py:665` calls are variadic and would break. These come from the lines I saw. Medium on the test impact and on the `AIMessageChunk.__add__` link, since I did not check them. Medium on completeness, because of the `libs/core` scope.