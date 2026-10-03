Prometheus writes a staleness marker, a sample whose value is the special `value.StaleNaN` NaN, for each series that was present in the previous scrape but is missing from the current one. All of this is in `scrape/scrape.go`. The same mechanism covers a series vanishing from a scrape, a failed scrape, and a target that stops being scraped.

**1. A series disappears from a scrape**
- The scrape cache keeps two maps, `seriesCur` and `seriesPrev` (`scrape.go:1020-1021`).
- While appending a scrape, each successfully appended series is recorded in `seriesCur` via `trackStaleness` (`:1163-1165`, called at `:1965` and `:1986`).
  - Series with an explicit exposed timestamp skip this tracking, unless `trackTimestampsStaleness` is set (`:1964`, `:1983-1984`).
  - Series that were never appended (for example, rejected by a sample limit) are not added to the cache, so they never get stale markers (`:1977-1981`).
- `forEachStale` (`:1167-1175`) returns every series that is in `seriesPrev` but not in `seriesCur`.
- `updateStaleMarkers` (`:1753-1768`) appends `math.Float64frombits(value.StaleNaN)` at the scrape timestamp for each of those series (`:1757`). It ignores out-of-order and duplicate-sample errors, because those are expected when a target comes back under a new scrape loop (`:1760-1763`).
- `iterDone` (`:1058`, swap at `:1102-1103`) then swaps `seriesPrev` and `seriesCur` and clears the new `seriesCur`.

**2. A failed scrape or a parse error**
- A failed scrape is treated like an empty scrape. `app.append` is still called, which triggers the stale markers (`:1610-1613`).
- An empty body takes the `len(b) == 0` branch of `append`, which calls `updateStaleMarkers` and `iterDone(false)`, so the cache is swapped but not flushed (`:1781-1785`).
- If the append itself fails, for example on a parse error or sample limit, the code rolls back and calls `append` again with an empty body (`:1619-1629`).
- A forced error does the same thing (`:1544-1551`).

**3. A target stops being scraped**
- When the loop's `run` exits, a deferred function runs (`:1409-1421`).
  - It optionally does a final scrape if `scrapeOnShutdown` is set.
  - It closes `sl.stopped`.
  - If `parentCtx` is not cancelled and end-of-run markers are not disabled, it calls `endOfRunStaleness` (`:1415-1418`).
- `endOfRunStaleness` (`:1662-1728`) does the following:
  - It returns without writing anything if there was never a scrape (`:1670-1673`).
  - It returns if `parentCtx` is cancelled, which is treated as server shutdown (`:1666-1668`, `:1678`).
  - It waits one ticker tick and records that time as `staleTime`, waits a second tick, then waits an extra `interval/10`. This gives a recreated target time to scrape first (`:1675-1697`).
  - It returns if end-of-run markers were disabled during the wait (`:1699-1702`).
  - It calls `app.append([]byte{}, "", staleTime)` with an empty body, which writes StaleNaN for every series in the cache. It then calls `sl.reportStale` for the report series and commits (`:1704-1727`).
  - If the target was recreated and scraped meanwhile, the markers arrive out of order and are dropped (`:1705-1706`).
- `disableEndOfRunStalenessMarkers` (`:1737`) sets the atomic flag that turns this off. I did not trace its callers.

**Not verified:** I did not read the `Pool.Sync` or `reload` code that decides which loops to stop (around `:483`, `:585`). I also did not trace what calls `disableEndOfRunStalenessMarkers`.