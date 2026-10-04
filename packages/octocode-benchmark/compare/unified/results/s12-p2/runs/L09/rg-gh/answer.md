Prometheus marks a series stale by appending a special NaN sample, `value.StaleNaN`, for it. The scrape loop does this in two situations: when the series is missing from a scrape, and when the loop stops. Everything below is in `scrape/scrape.go` unless noted. I read the code but did not run it or look at tests.

**Stale marker value**
- `model/value/value.go:24-33` defines `StaleNaN = 0x7ff0000000000002`, a signaling NaN, and `IsStaleNaN` tests for it.

**A series disappears from a scrape**
- The scrape cache keeps two maps of series seen: `seriesPrev` (previous scrape) and `seriesCur` (current scrape) (`:1019`).
- While appending a scrape, each series is recorded in `seriesCur` via `trackStaleness` (`:1163-1165`). The calls are at `:1964-1965` and `:1983-1986`.
- Series with explicit timestamps are skipped unless `trackTimestampsStaleness` is set (`:1964`, `:1983`).
- After a successful append, `updateStaleMarkers` runs (`:2082`). It calls `cache.forEachStale` (`:1167-1175`), which yields every ref in `seriesPrev` that is not in `seriesCur`.
- For each of those, `updateStaleMarkers` appends `math.Float64frombits(value.StaleNaN)` at the scrape timestamp (`:1753-1759`).
- Out-of-order and duplicate-timestamp errors are ignored there (`:1760-1764`). They are expected when a target goes away and comes back with a new scrape loop.

**A scrape fails**
- A failed scrape or failed append is treated as an empty scrape. The loop calls `app.append([]byte{}, "", ts)`, so nothing is in `seriesCur` and every previous series gets a marker (`:1611`, `:1624-1626`).
- The same empty-append happens for a forced error at `:1546-1549`.
- `append` with an empty body only runs `updateStaleMarkers` and then `cache.iterDone(false)` (`:1780-1785`).

**A target stops being scraped**
- When the loop exits, it calls `endOfRunStaleness` unless `disabledEndOfRunStalenessMarkers` is set (`:1416-1417`).
- `endOfRunStaleness` (`:1662-1728`) does nothing if there was never a scrape (`:1670-1673`) or if the parent context is cancelled, which is assumed to mean server shutdown.
- It takes `staleTime` at the next ticker tick, waits one more tick, then waits an extra `interval/10`. This gives a recreated target time to scrape first (`:1675-1697`).
- It re-checks the disable flag (`:1699-1702`). It then does an empty `app.append`, which writes `StaleNaN` for all series, and calls `reportStale` (`:1704-1727`).
- `reportStale` (`:2330-2360`) also writes stale markers for the synthetic series (`up`, `scrape_duration_seconds`, `scrape_samples_scraped`, and similar).
- If the target was recreated and scraped in the meantime, the late markers are out of order and get dropped (`:1705-1707`).

**Suppressing the markers when a loop is replaced**
- `disableEndOfRunStalenessMarkers` (`:1737-1738`) sets the flag. It is called on the old loop at `:364` (during a pool reload) and from `scrapePool.disableEndOfRunStalenessMarkers(targets)` at `:605-610`. In those cases the new loop takes over the same series.

**Not verified**
- I did not check which callers invoke `scrapePool.disableEndOfRunStalenessMarkers`.
- `git` was not usable here, so I did not confirm that the checkout is at the pinned commit `ea954809ce`.