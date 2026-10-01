Prometheus writes a sample whose value is the special `StaleNaN` NaN into the series. It does this in two cases: when a scrape no longer exposes a series, and when a target's scrape loop stops. All of this is in `go/scrape/scrape.go` at the pinned commit. I did not read the full `append()` body (lines ~1738–2317), and I did not confirm that a failed scrape goes through the same path (see the last section).

**1. A series disappears from a scrape (or the scrape is empty or failed)**
- The scrape cache keeps two maps of series references, `seriesPrev` and `seriesCur`. `trackStaleness(ref, ce)` adds each series seen in the current scrape to `seriesCur` (`scrape.go:1163`).
- `forEachStale` visits every series that is in `seriesPrev` but not in `seriesCur` (`scrape.go:1188-1196`).
- `updateStaleMarkers` (`scrape.go:1753`) appends `math.Float64frombits(value.StaleNaN)` for each of those series at the scrape's default timestamp (`scrape.go:1757`). The comment there reads "Series no longer exposed, mark it stale."
- Out-of-order and duplicate-timestamp errors are ignored, since they are expected when a target goes away and comes back with a new loop (`scrape.go:1759-1763`). It uses `aOptionRejectEarlyOOO`.
- In `append()`, an empty body (`len(b) == 0`) only calls `updateStaleMarkers` and then `cache.iterDone(false)`, which swaps the cache without flushing it (`scrape.go:1782-1786`). That call is what marks every previously seen series stale.
- A non-empty scrape swaps the cache in a deferred `iterDone(true)` (`scrape.go:1833-1837`). The same logic exists for the v2 appender in `scrape_append_v2.go:68` (`updateStaleMarkersV2`).
- The scrape's own report series (`up` and the like) get stale markers through `reportStale` (next section).

**2. A target stops being scraped**
- When the loop's main function exits, it calls `endOfRunStaleness(last, ticker, interval)` (`scrape.go:1416-1417`). It skips this if the parent context is cancelled (server shutdown) or if `disabledEndOfRunStalenessMarkers` is set.
- `endOfRunStaleness` (`scrape.go:1662`) returns at once if there was never a scrape (`last.IsZero()`). Otherwise:
  1. It waits one ticker tick and records `staleTime`.
  2. It waits another tick, then an extra `interval/10` (`scrape.go:1676-1698`). The wait is there in case the target gets recreated.
  3. It re-checks the disabled flag.
  4. It calls `app.append([]byte{}, "", staleTime)`, an empty scrape that stales every series. It then calls `reportStale`, which appends `StaleNaN` for `up`, `scrape_duration_seconds`, `scrape_samples_scraped`, `scrape_samples_post_metric_relabeling`, `scrape_series_added` and, if `reportExtraMetrics` is set, the extra metrics (`scrape.go:1720-1725`, `scrape.go:2330-2350`).
- If the target was already recreated and scraped again, the stale samples are older than the new data, so storage rejects them as out of order and they are ignored (comment at `scrape.go:1718-1719`).
- `manager.go:559` calls `sp.disableEndOfRunStalenessMarkers(targets)`, which stops these markers for the targets it is given. I did not read the surrounding function to see when this is used.

**Not verified**
- A comment at `scrape.go:1610` ("A failed scrape is the same as an empty scrape,…") is cut off in what I read. It implies a failed scrape is treated as empty and so stales its series, but I did not read the lines that follow.
- I did not read the `target.go` uses of `IsStaleNaN` (lines 388, 404 and 519). I also did not read the handling of samples that carry explicit timestamps.