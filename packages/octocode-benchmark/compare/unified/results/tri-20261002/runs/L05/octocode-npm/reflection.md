1. **Helped:** My only call was `localSearch` (text mode, `merge_content`, scoped to `libs/core`). It returned the declaration at `base.py:366`, every call site, the `__init__.py` exports and the tests in one pass. The `callsite`/`declaration` kinds and the `*(o.content ...)` snippets showed which calls were variadic.

2. **Did not help:** I made no exact reads, so some claims were not verified.
   - I named `BaseMessageChunk.__add__` for `base.py:442` and `:453` from the snippet context, not from a read.
   - I named `add_ai_message_chunks` for `ai.py:665` without seeing that name. The snippet showed only the end of a docstring, so that name may be wrong.
   - I did not read `test_merge_content`'s parametrized cases, so whether any case would fail is unknown.
   - I ran no `lspGetSemantics` references. The search was lexical and limited to `libs/core`.

3. **Next time:** After the search, I'd batch `localGetFileContent` reads of `base.py` around 366–460, `ai.py` around 640–670, and `test_messages.py` around 1060–1110. I'd also run `lspGetSemantics` references on `merge_content`, and search the rest of the repo.

4. **Confidence:** Medium. The set of call sites and their argument shapes is well supported. The enclosing function names and the test impact were inferred, and I should have marked the `ai.py` function name as unverified in my answer.