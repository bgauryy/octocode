Prometheus marks a series stale by appending a sample whose value is the special `StaleNaN` float. This happens in two cases: a series that was in the previous scrape is missing from the current one, or the scrape loop for a target ends. All lines below are in `scrape/scrape.go`.

**1. A series disappears from a scrape (or the scrape fails)**
- The scrape cache keeps two maps, `seriesCur` and `seriesPrev`. While a scrape is appended, each series that was successfully appended with no explicit timestamp is added to `seriesCur` by `trackStaleness` (lines 1163-1165, called at 1965 and 1986).
- Series with an explicit timestamp are skipped, unless `trackTimestampsStaleness` is set (1983-1984).
- A new series that wasn't appended, for example because of `sample_limit`, is never cached, so it never gets a stale marker (1977-1980).
- At the end of the append, `updateStaleMarkers` runs (called at 2082, only if `err == nil`). It calls `cache.forEachStale`, which yields every ref in `seriesPrev` that is not in `seriesCur` (1167-1175).
- For each such series, `updateStaleMarkers` calls `app.Append(ref, lset, defTime, math.Float64frombits(value.StaleNaN))` (1753-1757). `ErrOutOfOrderSample` and `ErrDuplicateSampleForTimestamp` are ignored, because they are expected when a target is recreated (1759-1764).
- After a non-empty scrape, `iterDone(true)` flushes stale cache entries. Whether or not it flushes, it swaps `seriesPrev` and `seriesCur` and clears the new `seriesCur` (1101-1103, deferred call at 1829-1835).
- A failed scrape is treated as an empty one. The loop calls `append` with an empty body "to trigger stale markers" (1610-1613). If the append itself fails, for example on a parse error or sample limit, it rolls back and calls `append` again with an empty body (1619-1625).
- With an empty body, every previous series is stale. `append` runs `updateStaleMarkers` and then `iterDone(false)`, so the cache is swapped but not flushed (1781-1785).

**2. A target stops being scraped**
- When the loop exits, it calls `endOfRunStaleness` (1414-1418). It does so only if the parent context isn't cancelled and `disabledEndOfRunStalenessMarkers` is false. A cancelled context is treated as a server shutdown, so no markers are written.
- `endOfRunStaleness` (1662-1728) does nothing if there was never a scrape (1670-1673). Otherwise it:
  - waits one ticker interval and records that time as `staleTime`;
  - waits a second interval, then a further `interval/10` (1675-1697), so a recreated target has time to scrape first;
  - re-checks that the disabled flag is still false (1699-1702);
  - calls `app.append([]byte{}, "", staleTime)` with an empty scrape, which produces `StaleNaN` for every series in `seriesPrev` (1720);
  - calls `reportStale` (1725), which writes `StaleNaN` for the synthetic report series such as the health metric `up` (2330-2335).
- If the target is recreated and scraped again, the late stale markers come out as out-of-order or duplicate and are ignored.
- The end-of-run markers are turned off when the target is not really going away:
  - when a loop is replaced and its cache reused (364, which hands the cache to the new loop);
  - in `scrapePool.disableEndOfRunStalenessMarkers`, which sets the flag on loops for the given targets (605-611). `manager.go:559` calls it.

I did not read the body of `reportStale` past its first lines, or the parts of `manager.go` around line 559.