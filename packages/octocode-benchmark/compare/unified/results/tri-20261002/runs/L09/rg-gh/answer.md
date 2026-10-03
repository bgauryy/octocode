Prometheus writes a special `StaleNaN` sample (`value.StaleNaN`) for each series that was in the previous scrape but is missing from the current one. It does this in two situations. One is a series disappearing from a scrape, or the scrape failing. The other is a target stopping, which is handled at the end of the scrape loop. All paths below are in `scrape/scrape.go`.

**1. A series disappears from a scrape, or the scrape fails**
- **Tracking:** the `scrapeCache` keeps `seriesCur` and `seriesPrev` maps (`:1019-1021`). When a newly appended series gets a cache entry, `trackStaleness` adds it to `seriesCur` (`:1980-1990`, `:1163-1165`). If the sample has an explicit timestamp, it is skipped unless `trackTimestampsStaleness` is set.
- **Detection:** `forEachStale` (`:1167`) visits every ref in `seriesPrev` that is not in `seriesCur`.
- **Marking:** `updateStaleMarkers` (`:1753-1771`) appends `math.Float64frombits(value.StaleNaN)` at the scrape timestamp for each of those series.
  - It sets `aOptionRejectEarlyOOO` on the append.
  - It ignores `ErrOutOfOrderSample` and `ErrDuplicateSampleForTimestamp`. These are expected when a target goes away and comes back under a new scrape loop.
- **Successful scrape:** `updateStaleMarkers` runs at the end of the normal append (`:2081-2082`). A deferred `cache.iterDone(true)` then swaps `seriesCur` into `seriesPrev` and flushes old cache entries (`:1058-1095`, `:1834`).
- **Failed scrape:** the loop calls `append([]byte{}, "", ts)`, an empty scrape. With empty input, `append` only calls `updateStaleMarkers` and then `iterDone(false)`, so every previously seen series gets a marker (`:1781-1785`). This empty-scrape call is made in three cases:
  - when a forced error is set (`:1544-1547`);
  - when the scrape itself fails (I did not read the surrounding branch, so I'm inferring this from the call at `:1625`);
  - when the append of a scraped body fails, for example on a parse error or sample limit (`:1620-1626`).

**2. A target stops being scraped**
- **Trigger:** when the scrape loop exits, a deferred function calls `endOfRunStaleness`. This only happens if the parent context is not cancelled and end-of-run markers have not been disabled (`:1408-1418`).
- **Waiting:** `endOfRunStaleness` (`:1662-1730`) does nothing if there was never a scrape, and it exits if the parent context is cancelled (a server shutdown). Otherwise it:
  - waits for one ticker tick and records that time as `staleTime`;
  - waits for a second tick, then an extra `interval/10`. This gives the target time to be recreated and scraped by a new loop, in which case the markers would be out of order and ignored.
- **Marking:** it then does an empty `app.append([]byte{}, "", staleTime)`, which writes the per-series `StaleNaN` markers (`:1720`). It also calls `reportStale` (`:1724`), which writes `StaleNaN` for the synthetic series `up`, `scrape_duration_seconds`, `scrape_samples_scraped` and `scrape_samples_post_metric_relabeling` (`:2330-2345`).
- **Opting out:** `disableEndOfRunStalenessMarkers` sets an atomic flag (`:1737`). It is called from `scrapePool` when a loop is replaced (`:364`, `:605-610`) and from `scrape/manager.go:559`. I did not trace the exact conditions under which each of those fires.

**Uncertainty:** I read the code but did not run it or read the tests. I did not check which callers invoke the `manager.go:559` path.