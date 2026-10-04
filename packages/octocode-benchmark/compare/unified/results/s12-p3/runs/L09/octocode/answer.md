Prometheus marks a series stale by appending a sample whose value is the special `value.StaleNaN` bit pattern. It does this in two situations: when a series is missing from a scrape, and when the scrape loop for a target ends. All lines below are in `scrape/scrape.go` at the pinned commit.

**1. A series disappears from a scrape**
- The scrape cache keeps two maps, `seriesCur` and `seriesPrev` (lines 1020–1021). While parsing a scrape, each series that appears is recorded with `trackStaleness` (called at 1965 and 1986; it does `c.seriesCur[ref] = ce` at 1164).
- After the scrape, `updateStaleMarkers` (1753–1768) runs `cache.forEachStale`. `forEachStale` (1167–1175) yields every series that is in `seriesPrev` but not in `seriesCur`.
- For each of those series it appends `math.Float64frombits(value.StaleNaN)` at the scrape timestamp (1757). It uses the `aOptionRejectEarlyOOO` option for that append.
- Out-of-order and duplicate-timestamp errors are ignored (1760–1764). The comment says this is expected when a target goes away and comes back under a new scrape loop.
- `updateStaleMarkers` is called at the end of a successful, non-empty `append` (line 2082, only `if err == nil`). `iterDone(true)` then swaps the maps and flushes the cache (1834).
- The same logic exists for the V2 appender: `updateStaleMarkersV2` in `scrape_append_v2.go:68`, called at lines 96 and 401.

**2. A failed scrape**
- A failed scrape goes through the same `append` as a normal one (comment at 1610–1611, call at 1613). For a failed scrape, `b` is empty.
- An empty `b` takes the early branch at 1781–1786. That branch calls `updateStaleMarkers`, so every previously seen series gets a stale marker. It then calls `iterDone(false)`, which swaps the maps without flushing the cache.
- The same empty-append is used when a forced error is set (1547) and when an append fails, for example on a parse error or sample limit (1625).

**3. A target stops being scraped**
- When the loop exits, it calls `sl.endOfRunStaleness(last, ticker, sl.interval)` (1416–1417). This is skipped if `disabledEndOfRunStalenessMarkers` is set.
- `endOfRunStaleness` (1662–1728) returns early in two cases: no scrape ever happened (`last.IsZero()`, 1670), or the parent context was cancelled, which is treated as server shutdown (1678).
- Otherwise it waits two ticker intervals plus 10% of the interval, in case the target is recreated. The stale timestamp is taken at the first tick (1681).
- It then calls `app.append([]byte{}, "", staleTime)` (1720). This is the empty-scrape path above, so it marks every series in `seriesPrev` stale.
- It also calls `reportStale` (2330). This writes StaleNaN for the synthetic series `up`, scrape duration, samples scraped, samples post-relabel and series added (2335–2349). It writes the extra report metrics too if `reportExtraMetrics` is set (2350).
- If the target is recreated and scraped again in the meantime, the stale markers are out of order and get dropped (comment at 1704–1706).

**Disabling end-of-run markers**
- `disableEndOfRunStalenessMarkers` is called when a loop is replaced and its cache reused, at line 364 when `reuseCache` is true. The new loop then inherits the old loop's series state.
- It is also called through `scrapePool.disableEndOfRunStalenessMarkers` (605–613), which `manager.go:559` invokes.

**Uncertainty**
- I did not read the lines of `scrape.go` between 1177 and 1775 (other than the ranges above) or the part of `manager.go` around line 559. So I don't know what condition triggers the call at `manager.go:559`.