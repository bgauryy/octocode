1. **Helped:** The single `localSearch` call (text search for `should_redirect_with_slash|APPEND_SLASH`, excluding tests and locale) found `middleware/common.py` and the other `APPEND_SLASH` sites in one pass. The following `localGetFileContent` read of `common.py` lines 34-135 gave the complete logic with exact line numbers. Two calls were enough.

2. **Did not help:**
   - The search output was verbose, and the result flagged `partial: true`.
   - Match line numbers in the snippets are ambiguous, because the `line` field points at the match but the value includes context lines. I cited `global_settings.py:242`, `locale.py:59`, `sites.py:470`, `resolvers.py:173` and `decorators/common.py:7` straight from snippets without reading the files. Some may be off by one or two lines. Only the `common.py` citations were read directly.
   - I never read `is_valid_path`, `get_full_path`, `response_redirect_class` or `no_append_slash`. I also never confirmed the checkout is really at commit 4fab678a07, since I had no git access.

3. **Next time:** I would run a `localGetFileContent` batch on the secondary files to confirm their lines. I would also read the `response_redirect_class` definition (the first 33 lines of `common.py`), so I could state the redirect status rather than assume 301.

4. **Confidence:** High on the core mechanism, which I read directly. Medium on the secondary line citations and the redirect status code.