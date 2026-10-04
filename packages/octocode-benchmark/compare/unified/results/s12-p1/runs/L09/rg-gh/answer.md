Prometheus marks a series stale by appending a special `StaleNaN` sample for it. It does this in two situations: the series is missing from a scrape, or the target's scrape loop ends. All line numbers below are in `scrape/scrape.go`. I read the code but did not run it, and the checkout's git SHA couldn't be confirmed because `git` failed in the sandbox.

**1. A series disappears from a scrape**
- Each `scrapeLoop` has a `scrapeCache`. It tracks the series seen in the current scrape (`seriesCur`) and in the previous scrape (`seriesPrev`) (`:1019`, `:1163`).
- While appending a scrape, each series that was ingested is recorded with `trackStaleness` (`:1964-1965`, `:1983-1986`). Series with explicit timestamps are skipped unless `trackTimestampsStaleness` is set.
- `forEachStale` (`:1167`) yields every series that is in `seriesPrev` but not in `seriesCur`.
- `updateStaleMarkers` (`:1753-1758`) appends `math.Float64frombits(value.StaleNaN)` for each of those series. The sample is written at the scrape timestamp, and the comment there reads "Series no longer exposed, mark it stale". Out-of-order and duplicate-sample errors are ignored.
- `append` calls `updateStaleMarkers` at the end of a normal scrape (`:2082`). It also calls it for an empty body (`:1782-1783`).
- `iterDone` then swaps `seriesPrev` and `seriesCur` and clears the new current map (`:1100-1103`).
- A failed scrape also produces markers:
  - A failed scrape is treated as an empty scrape (`:1611`).
  - A forced error, such as a target limit, runs an empty append (`:1546-1547`).
  - If the append itself fails, it is retried with an empty body (`:1624`).
- `scrape_append_v2.go:69-71` has the same logic for the v2 appender.

**2. A target stops being scraped**
- When `scrapeLoop.run` exits, its deferred function calls `endOfRunStaleness` (`:1411-1418`). It skips this if the parent context was cancelled, which is the server-shutdown case, or if end-of-run markers were disabled.
- `endOfRunStaleness` (`:1662`) returns early if the loop never scraped (`:1670-1673`).
- Otherwise it waits two ticker intervals plus 10% of the interval. This gives a recreated target time to scrape first (`:1675-1697`).
- It rechecks that markers are still enabled (`:1700`). It then appends an empty scrape at `staleTime`, which makes every series in `seriesPrev` stale (`:1720`).
- It also calls `reportStale` (`:1725`), which writes `StaleNaN` for the synthetic series. These are `up`, `scrape_duration_seconds`, `scrape_samples_scraped` and the other report metrics (`:2330-2357`).
- If the target comes back, the stale markers are out of order and get ignored (`:1704-1706`).
- Markers are suppressed when a pool reload reuses the cache. `oldLoop.disableEndOfRunStalenessMarkers()` is called then (`:364`), and `scrapePool.disableEndOfRunStalenessMarkers` does the same for given targets (`:605-611`).

I didn't look at how the TSDB handles `StaleNaN` after it is written.