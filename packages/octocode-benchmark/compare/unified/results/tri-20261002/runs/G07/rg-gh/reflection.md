**1. Helped:**
- The first `gh api ... contents/starlette/applications.py?ref=63c5760d8a | base64 -d | grep -n` call gave me the stack outline in one step. It also confirmed the short SHA works as a ref.
- The second call defined a `g()` helper that ran `gh api` through `base64 -d | cat -n`. With `sed -n` ranges it fetched `applications.py`, `exceptions.py`, `_exception_handler.py` and `errors.py` together. That gave me exact line numbers for citations.

**2. Did not help:**
- My opening `ls` of the local repos directory failed with "Operation not permitted", so I wasted the guess that a local checkout existed.
- Every shell call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless but noisy.
- The first grep had no context lines, so I needed the second call for citable ranges.
- I never read the `ServerErrorMiddleware` constructor or `Router`.

**3. Next time:** Skip the local `ls` and go straight to `gh api` with `cat -n`. Fetch `applications.py` and the middleware files in a single call. If time allowed, `rg`-style checks through `gh api` for `wrap_app_handling_exceptions` callers, such as routing and `BaseHTTPMiddleware`.

**4. Confidence:** High on assembly order and handler lookup, because I read the code directly at the pinned ref. Medium on completeness, because I didn't check the router and `BaseHTTPMiddleware` paths, and I said so in the answer.