1. **Helped:** The first Bash call was the most useful. It ran `git log -1` to confirm the pinned SHA, and one `rg` for `StaleNaN|endOfRunStaleness|forEachStale|iterDone` gave me the whole map of `scrape.go`. Reading the `sed` ranges around `updateStaleMarkers`, `endOfRunStaleness`, `append` and `iterDone` then confirmed the mechanism directly.

2. **Did not help:** I did not filter the `rg` output and cut it with `head`, so some matches were noise (the `stopped` and `sl.cancel` hits). Two `sed` calls read regions I had already seen or didn't need. I never opened `manager.go` around line 559, so the claim about which callers disable end-of-run markers rests on an `rg` hit only. I didn't need the `gh` CLI, since the local checkout was enough.

3. **Next time:** I'd run one `rg` with `-n -C3` on the key symbols. I'd also read `manager.go` and the `sync` code path at the start, so the "target stops being scraped" answer covers the pool and manager side, not just the loop side.

4. **Confidence:** High for the core mechanism, because I read the cited lines directly. Medium for the reload and `disableEndOfRunStalenessMarkers` details, because I didn't read the callers.