**1. Helped:**
- `localSearch` for "unsaved related object" found the error message in `base.py` in one call, and its `next.fetch` hint pointed at the right lines.
- The `localGetFileContent` read of lines 1305–1350 and the follow-up read of 1276–1304 gave the exact check logic.
- The `localSearch` for `_prepare_related_fields_for_save` with `resultView: detailed` found all three call sites.

**2. Did not help:**
- My first search used the full message string, which wraps across two source lines, so it returned nothing.
- The `matchString` read returned a truncated "401 lines omitted" view, so I needed another read.
- The detailed search snippets begin a few lines before the matched line, and I misread that offset.

**3. Next time:** search a short message fragment first. Read the call-site lines directly instead of inferring them from snippet layout.

**4. Confidence:**
- **High:** the mechanism and the line numbers at `base.py:1276–1303` and `1317–1330`, all read directly.
- **Correction:** I cited `query.py:797` and `1045` for the bulk call sites. The search reported `794` and `1042`, and the snippets start before the match line. I believe 794 and 1042 are the call lines and my numbers are off by three, but I did not read them directly.
- **Unchecked:** the re-sync code after line 1304 was only partly read.