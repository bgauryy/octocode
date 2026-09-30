I have what I need and I'm writing up the answer. All references are to `scrape/scrape.go` at ea954809ce.

**Answer:** Prometheus writes a `StaleNaN` sample (`value.StaleNaN`) for each series that was present in the previous scrape but is missing from the current one. It does this for two triggers:
- **Series that disappear from a scrape:** the scrape loop compares the current scrape against the previous one and writes the markers immediately.
- **A target that stops being scraped:** the loop's shutdown path appends an empty scrape after a delay. This runs the same comparison with nothing "current", so every series gets a marker.

**1. Tracking which series were seen**
- `scrapeCache` keeps `seriesCur` and `seriesPrev`, both keyed by series ref (`newScrapeCache`, around line 1076).
- `trackStaleness` adds a series to `seriesCur` (line 1163). It is called for cached series at line 1965 and for newly added series at line 1986.
- Series with explicit exposition timestamps are skipped unless `trackTimestampsStaleness` is set (lines 1965 and 1985). They therefore never get stale markers.
- Series that failed to append, for example because of `sample_limit`, are not put in the cache (comment at line 1980), so they never get a marker either.
- `forEachStale` (line 1167) yields every ref in `seriesPrev` that is not in `seriesCur`.
- `iterDone` (line 1058) swaps the two maps and clears the new `seriesCur` (lines 1104–1106).

**2. Series that disappear from a scrape**
- `append` handles the normal case. After a non-empty scrape, `updateStaleMarkers` (line 1753) runs `forEachStale`. It calls `app.Append(ref, lset, defTime, math.Float64frombits(value.StaleNaN))` (line 1757).
- Out-of-order and duplicate-sample errors are ignored there (lines 1758–1762). This is expected when a target goes away and comes back under a new scrape loop.

**3. Failed scrapes and empty bodies**
- A failed scrape is treated as an empty scrape. `append` is still called (comment and call at lines 1607–1609).
- With an empty body, `append` calls `updateStaleMarkers` and then `iterDone(false)` (lines 1782–1785). Every series in the previous scrape therefore gets a stale marker, and the cache is not flushed.
- If the append itself fails, for example on a parse error or sample limit, the code retries with an empty append to write stale markers (lines 1620–1627).
- `reportStale` (line 2330) is the separate path that marks the synthetic report series stale (`up`, `scrape_duration_seconds`, `scrape_samples_scraped` and the others).

**4. Target stops being scraped**
- The deferred function in `run` (around line 1405) calls `endOfRunStaleness` after the loop stops. It is skipped if the parent context was cancelled, which is treated as server shutdown, or if `disabledEndOfRunStalenessMarkers` is set.
- `endOfRunStaleness` (line 1662) returns early if there was never a scrape (`last.IsZero()`).
- It waits for one ticker tick and records `staleTime`. It then waits another tick and an extra `interval/10` (lines 1677–1698). This gives a recreated target time to ingest samples first.
- It then appends an empty scrape (`app.append([]byte{}, "", staleTime)`, line 1713), which goes through the same path as in section 3.
- It also calls `reportStale(app, staleTime)` (line 1725) to mark the report series stale.
- If the target was recreated and scraped in the meantime, the stale markers are out of order and get ignored (comment at lines 1704–1706).

**5. Suppressing markers when a loop is replaced**
- When a pool reload reuses the cache, the old loop's end-of-run markers are disabled (`oldLoop.disableEndOfRunStalenessMarkers()`, line 364). The new loop continues from the same cache and marks series stale itself if they vanish.
- `scrapePool.disableEndOfRunStalenessMarkers(targets)` (line 605) disables them per target. It is called from `manager.go:559`.

**Uncertainty:** I did not open the code around line 559 in `manager.go` or the body of `scrapePool.disableEndOfRunStalenessMarkers`. I also did not trace `sync` (around line 555) beyond seeing that it calls `l.stop()`. I did not verify which callers trigger `disableEndOfRunStalenessMarkers` beyond the two sites above.