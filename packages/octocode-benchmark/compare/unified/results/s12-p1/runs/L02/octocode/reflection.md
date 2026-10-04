1. **Helped:** The second `localSearch`, for the literal "prohibited to prevent data loss" with `contextLines: 12`, found the `ValueError` and the surrounding check (`base.py:1289-1313`) in one call. The next two calls ran in parallel: a `localFetch` of `base.py:1268-1290` for the function head and comment, and a `localSearch` for `_prepare_related_fields_for_save` that listed the callers. Searching a short literal and reading around the hit was the efficient pattern.

2. **Did not help:**
   - My first `localSearch` was wasted. I guessed the full message including "save()", but the source builds it with `%s`, so nothing matched. I also passed `regex: "literal"`, which may not be a valid option.
   - I did not read the `query.py` call sites, including `query.py:1042`, so I could not say which operation that one is. The answer says so.
   - The tools had no problems otherwise. Output was clean.

3. **Next time:** Search only the stable fragment of the message from the start. Add a `matchString` or small range read for `query.py:1042`. A `lspSearch` references query on the method would also confirm the caller list without relying on text search.

4. **Confidence:** High. Every claim comes from lines I saw at the pinned checkout, and the only gap is the unread call site.