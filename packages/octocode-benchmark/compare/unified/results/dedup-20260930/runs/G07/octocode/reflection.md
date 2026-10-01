**1. Helped:** The first parallel batch of three `ghGetFileContent` calls did most of the work. I pinned the branch to `63c5760d8a` and read `applications.py` with `matchString: "def build_middleware_stack"`, plus `exceptions.py` and `_exception_handler.py` with `fullContent`. That covered assembly order and handler dispatch in one round. The second `ghGetFileContent` call on `errors.py` with `matchString: "async def __call__"` returned the `ServerErrorMiddleware` behavior without reading the 259-line file.

**2. Did not help:**
- My `ghSearchCode` call for `wrap_app_handling_exceptions` failed validation because I passed `keywords` as a string instead of an array. I didn't retry, so a question I'd already raised went unanswered: whether routes also wrap with it.
- The `matchString` window in `applications.py` returned no line numbers for `build_middleware_stack` and `__call__`. I could only say "roughly line 63", which is weak evidence. A small explicit `startLine`/`endLine` read would have given exact lines.
- The `errors.py` match window started in HTML template text, which was noise.

**3. Next time:** I'd include the retry of the search, or a direct read of `routing.py`, in the same batch so the router question was settled. I'd also re-read the key functions by line range so every citation has a line number.

**4. Confidence:** High on stack order and the `ExceptionMiddleware` and `ServerErrorMiddleware` flow, since I saw that source directly. Medium on completeness, because I never read the router or `RequestBodyLimitMiddleware`, and some line numbers are missing.