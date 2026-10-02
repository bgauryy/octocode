Prometheus writes a `StaleNaN` sample (`value.StaleNaN`) for each series that was present in the previous scrape but is missing from the current one. For a target that stops being scraped, the scrape loop does the same thing with an empty scrape after a short delay. All paths below are in `scrape/scrape.go` at ea954809ce, except where noted.

**1. A series disappears from a scrape**
- **Tracking (scrapeCache):** `scrapeCache` keeps two maps, `seriesCur` and `seriesPrev`.
- **Recording series:** When a sample is appended successfully, `sl.cache.trackStaleness(ce.ref, ce)` adds it to `seriesCur` (`scrape.go:1964-1966`, definition at `:1163-1165`). This happens only if the sample has no explicit timestamp, or if `trackTimestampsStaleness` is set.
- **Finding stale series:** `forEachStale` (`:1167-1175`) walks `seriesPrev` and calls back for each ref that is not in `seriesCur`.
- **Writing the marker:** `updateStaleMarkers` (`:1754-1757`) calls `forEachStale` and does `app.Append(ref, lset, defTime, math.Float64frombits(value.StaleNaN))`. It sets `aOptionRejectEarlyOOO` around the append. `defTime` is the scrape timestamp. The appender-V2 path has an equivalent, `updateStaleMarkersV2`, in `scrape_append_v2.go:71`.
- **Swapping maps:** `iterDone` (`:1101-1103`) swaps `seriesPrev` and `seriesCur`, clears the new `seriesCur`, and increments `iter`. The next scrape therefore compares against this one.
- **Cache cleanup:** `iterDone(true)` also drops cache entries not seen in the last scrape (`:1080-1097`). That is housekeeping; it does not write markers.
- **Empty body:** An empty scrape body calls `updateStaleMarkers` and then `iterDone(false)` (`:1784-1786`), so every previously seen series gets a marker.
- **Failed scrapes:** I did not read the failed-scrape path in detail. The `reportStale` function at `:2332` writes stale markers for the synthetic report series such as the scrape health metric.

**2. A target stops being scraped**
- **Where it starts:** The scrape loop's deferred shutdown calls `sl.endOfRunStaleness(last, ticker, sl.interval)` (`:1409-1419`). It does so only if the parent context is not cancelled and `disabledEndOfRunStalenessMarkers` is false. A cancelled parent context is presumed to mean the server is shutting down, and no markers are written.
- **`endOfRunStaleness` (`:1662-1728`):**
  - It returns right away if there was never a scrape (`last.IsZero()`).
  - It waits for one ticker tick and records that time as `staleTime`.
  - It waits for a second tick plus `interval/10`. The wait exists because the target may be recreated.
  - It re-checks the disabled flag.
  - It calls `app.append([]byte{}, "", staleTime)`, an empty scrape that triggers `updateStaleMarkers` as above. It then calls `reportStale` and commits.
  - If the target was recreated and has already been scraped, the markers are out of order and get ignored (`:1704-1706`).
- **Opting out on reload:** `disableEndOfRunStalenessMarkers` (`:1737-1739`) sets the atomic flag. On a scrape-pool reload where the cache is reused, the old loop gets this call (`:364-366`). `scrapePool.disableEndOfRunStalenessMarkers` (`:605-612`) does the same for specific targets. `manager.go:559` calls that pool method.

**Uncertainty:** I read the reload code at `:364` only from a search snippet, not in full. I did not trace what `manager.go` does around line 559 in any detail. I did not read the scrape-error path that leads to the `:1784` empty-body case.