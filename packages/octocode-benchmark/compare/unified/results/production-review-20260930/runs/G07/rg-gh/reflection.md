1. **Helped:** The first Bash call was the most useful. The loop of `gh api repos/Kludex/starlette/contents/starlette/$f.py?ref=63c5760d8a` with the raw Accept header pulled the four key files at the pinned commit. The `grep -n "def build_middleware_stack" -A45` on it gave the stack order and line numbers straight away. The second call (`cat -n` plus a grep of `errors.py`) gave the handler-lookup and 500-path evidence. The routing.py grep showed the per-route wrapping. Fetching single files avoided a full clone.

2. **Did not help:**
   - I wrote the files into `/tmp` with a `basename` loop. That directory already held many unrelated `.py` files, so `wc -l *.py` was noisy. It also risked overwriting files that were already there, such as `errors.py` or `imp.py`, which I didn't check first. That was careless.
   - The `_exception_handler` path went through `basename`, which worked only by luck.
   - I never read `middleware/__init__.py`, so the `Middleware` tuple unpacking is assumed rather than seen.

3. **Next time:** Use a fresh `mktemp -d` directory. Fetch `middleware/__init__.py` and `routing.py` in the same first call. Check whether the local repos already hold a copy before hitting GitHub.

4. **Confidence:** High for the stack order and the exception flow, because I saw every cited line. Medium-high on the `Middleware` unpacking and on routing.py details beyond the grep hits.