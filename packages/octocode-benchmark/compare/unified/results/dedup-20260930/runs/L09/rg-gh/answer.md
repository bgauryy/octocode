Prometheus marks a series stale by appending a special `StaleNaN` sample to it. It does this by comparing the series seen in the previous scrape with those seen in the current one. All line numbers below are in `scrape/scrape.go` at ea954809ce.

**1. A series disappears from a scrape**
- Each successfully appended sample is recorded in the cache's `seriesCur` map via `cache.trackStaleness(ref, ce)` (`scrape.go:1163-1165`). The calls are at `:1964-1965` and `:1983-1986`. They are skipped for samples with explicit timestamps unless `trackTimestampsStaleness` is set.
- `forEachStale` (`:1167-1175`) walks `seriesPrev` and yields every ref that is not in `seriesCur`.
- `updateStaleMarkers` (`:1753-1768`) appends `math.Float64frombits(value.StaleNaN)` for each of those series at the scrape's default timestamp. It ignores out-of-order and duplicate-timestamp errors, which are expected when a target is recreated with a new loop.
- `iterDone` (`:1058`) then swaps `seriesCur` into `seriesPrev` for the next round. I did not read the swap lines themselves, only the flush logic above them. After a non-empty scrape it runs as `iterDone(true)` (`:1834`), which also drops cache entries not seen in the last iteration (`:1076-1090`).

**2. A scrape fails or returns an empty body**
- A failed scrape is treated as an empty scrape (comment at `:1610-1611`). `app.append(b, ...)` is still called with empty `b`.
- When `len(b) == 0`, `append` calls `updateStaleMarkers` and then `iterDone(false)` (`:1779-1784`). That marks every series from the previous scrape stale and does not flush the cache.
- If the append itself fails, for example on a parse error or sample limit, the code rolls back and calls `append` again with an empty body (`:1621-1632`). This also produces the stale markers.

**3. A target stops being scraped (the scrape loop ends)**
- When `run` exits (`:1404-1420`), its deferred function calls `sl.endOfRunStaleness(...)`. It does this only if the parent context is not cancelled, so a server shutdown writes no stale markers. It also skips this if `disabledEndOfRunStalenessMarkers` is set.
- `endOfRunStaleness` (`:1662-1730`) behaves as follows:
  - It returns without writing anything if there was never a scrape (`last.IsZero()`).
  - It waits for one ticker tick and records that time as `staleTime`.
  - It then waits a second tick plus `interval/10`, so a recreated target has time to ingest its own samples.
  - It re-checks the disabled flag.
  - It calls `app.append([]byte{}, "", staleTime)`, which writes StaleNaN for all remaining series, as in case 1.
  - It calls `reportStale` (`:2330-2345`), which writes StaleNaN for the synthetic series `up`, `scrape_duration_seconds`, `scrape_samples_scraped`, `scrape_samples_post_metric_relabeling` and so on.
- When a scrape pool reloads and reuses the old loop's cache (`:360-366`), it calls `oldLoop.disableEndOfRunStalenessMarkers()`. This avoids spurious stale markers for targets that continue under a new loop. The `disableEndOfRunStalenessMarkers` method is at `:1737-1739`.

**Related detail:** `updateRef` (`:1138-1150`) moves staleness tracking to the new ref when the storage recreates a series. This stops such a series from being wrongly marked stale.

**Uncertainty:** I did not read the `iterDone` lines that swap `seriesCur` into `seriesPrev`. I also did not check the end-of-run path outside `scrape.go`, for example how the scrape pool stops loops when targets are removed. The `scrapePool` code around `:483` handles that, and I did not trace it.