**1. Helped:** The first `ghGetFileContent` call (matchString `def resolve_redirects`) put the main loop and `get_redirect_target`/`should_strip_auth` in one read. The range read `286,420` in the second batch then supplied `rebuild_auth`, `rebuild_proxies` and `rebuild_method`. The short SHA `611c6162cb` resolved and returned the full commit SHA, which let me pin the citations.

**2. Did not help:**
- `contextLines: 120` was clamped to 100, so the first read cut off at line 286. That cost a follow-up call.
- The second query in that batch used three matchStrings (`allow_redirects`, `gen = self.resolve_redirects`, `history`). It returned fragmented output with many "lines omitted" gaps and repeated lines I already had. It did give me `send` at `:773-822`, but a direct range read would have been cleaner.
- I did not verify `Response.is_redirect`, the `max_redirects` default, or `Response.next`. My statement that `r._next` backs `Response.next` is inferred from the comment at `:817`, not read from `models.py`.

**3. Next time:** Read explicit ranges (`186-310`, then `760-825`) instead of one large matchString read plus a noisy multi-literal search. Add one `models.py` read for `is_redirect` and `next`.

**4. Confidence:** High on the redirect loop, method, header, cookie, auth and proxy behaviour, since each point cites lines I read. Medium on the `Response.next` link.