**Helped:** The first `grep -n` across `scrape/*.go` for `StaleNaN|endOfRunStaleness|iterDone|forEachStale` located every relevant function in one call. The second call printed `iterDone`, `forEachStale`, `run`, `endOfRunStaleness` and `updateStaleMarkers` with line numbers. A final targeted `sed` batch covered the failed-scrape paths, the cache swap and the `disableEndOfRunStalenessMarkers` call sites.

**Did not help:**
- `rg` is not installed, so my first call failed and I fell back to `grep`.
- Every shell call printed `/dev/null: Operation not permitted`. This was noise, but the output was still usable.
- `git log -1` failed, so I could not confirm the checkout is at `ea954809ce`.
- The `grep` listed `scrape.go` twice because I passed it both explicitly and via the glob.
- I never opened `scrape_append_v2.go` beyond the grep hits, so the claim that it mirrors the v1 logic rests on those lines only.

**Next time:** Use `grep` from the start, pass a single path, and read `.git/HEAD` directly to verify the commit. I would also open the v2 file around lines 60-150.

**Confidence:** Medium-high on the mechanism, since I saw the code for each step. Medium on exact line numbers. Most come from printed `sed` and `grep -n` output. A few, like the `iterDone` swap at 1100-1103 and the `endOfRunStaleness` wait steps, I inferred from the starting offset of the `sed` range. I did not check the TSDB side.