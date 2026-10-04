1. **Helped:** The `grep -n -i "stale" scrape/scrape.go` call was the fastest step. It listed every staleness site with line numbers in one pass. The single `sed -n` call that printed the ranges around `trackStaleness`/`forEachStale`, `endOfRunStaleness`, `updateStaleMarkers` and `append`, plus a grep of `value.go`, gave me the mechanism and the `StaleNaN` definition.

2. **Did not help:**
   - The first call failed because `rg` was not installed, even though the brief said it was available, so I wasted a call.
   - Every shell call printed "/dev/null: Operation not permitted".
   - `git log -1` failed for the same reason, so I could not confirm the checkout was at `ea954809ce`. I said so in the answer.

3. **Next time:** I would try `grep` first. I would use `grep -n` or `cat -n` on the code ranges, because my `sed` output had no line numbers. Several cited ranges (about `:1546-1549`, `:1670-1673`, `:1705-1707`, `:1760-1764`, `:1780-1785`) came from counting offsets from the `sed` start lines, so they could be off by a few lines. I would also read the context around `:364` and the callers of `scrapePool.disableEndOfRunStalenessMarkers`. My "pool reload" label for `:364` and the "new loop takes over the same series" reasoning were inferred from names and comments.

4. **Confidence:** High on the overall mechanism (empty-append stale markers, `endOfRunStaleness`, `StaleNaN`), because I read the code directly. Medium on exact line numbers for the offset-counted ranges, and on the disable-flag context.