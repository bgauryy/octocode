1. **Helped:** The first batched `localSearch` (two queries, with `contextLines` and `include` globs) found `getGenericCommand` and `expireIfNeeded` at once. It also returned enough surrounding code to show the GET path. The `localFetch` batch then gave the full `expireIfNeeded` body and the `lookupKey` doc comment and code with only one `matchString`.

2. **Did not help:**
   - The first `localFetch` failed validation because I left out `reasoning`. That cost one wasted call.
   - `localSearch` returned line numbers for the match window rather than for the specific function. The `t_string.c` numbers were therefore imprecise, and I had to say "approximately" in the answer.
   - I never opened `lookupKeyReadOrReply`, `keyIsExpired` or `deleteExpiredKeyAndPropagate`. The GET-to-`lookupKeyRead` link is inferred, not verified.
   - The `lookupKey` line numbers were also not confirmed. The `matchString` fetch showed a source range of 206–386, and I didn't pin exact lines from it.

3. **Next time:** I would include `reasoning` and `goal` in every query from the start. I'd add a `lookupKeyReadOrReply` query to the first batch. I'd also fetch `t_string.c` with a `matchString` on `getGenericCommand` to get exact line numbers, or use `lspSearch` on the symbol.

4. **Confidence:** medium-high on the behavior, since the `expireIfNeeded` and `lookupKey` code I read supports it directly. Medium on the exact citations, since some line numbers are approximate or missing.