1. **Helped:** `localFetch` with `ranges` (1140-1260, then 1084-1140) gave numbered source lines for the whole decision. The first `localSearch` for `def _save_table` with `contextLines: 90` found the function and showed the `save_base` call site and `_save_parents`, though the output was truncated.

2. **Did not help:**
   - The `countLines` `localSearch` was wasted. It returned only a count and no line number.
   - The `localFetch` ranges call with a bare `"1084"` failed validation. I should have used `start-end` from the start.
   - The first search's reported match line (1075) did not line up with the content returned, which began in `save_base`. I had to confirm with a second `localFetch` using `matchString`, which put `def _save_table` at 1075.
   - I cited the `save_base` behavior (passing `force_insert or parent_inserted`) as "around line 1075". I never fetched that span with line numbers, so that citation is imprecise and probably wrong.

3. **Next time:** Start with `localFetch` and `matchString` to get exact line numbers, then fetch a single range covering `save_base` and `_save_table`. I would skip `countLines` and avoid wide `contextLines` on `localSearch`.

4. **Confidence:** High on the UPDATE-then-INSERT logic and the `NotUpdated` cases, which I read with line numbers at 1103-1212. Medium on the `save_base` line reference.