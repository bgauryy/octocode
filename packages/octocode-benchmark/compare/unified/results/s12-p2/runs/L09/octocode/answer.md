Prometheus writes a staleness marker (a float sample whose value is `value.StaleNaN`) for a series in two situations. One is when the series is missing from a scrape that has just been parsed. The other is when the target's scrape loop stops. All of this is in `scrape/scrape.go`.

**1. A series disappears from a scrape**
- The scrape cache keeps two maps, `seriesCur` and `seriesPrev` (`scrape.go:1020-1021`). While a scrape is being appended, every series that is appended, whether cached or new, is recorded in `seriesCur` through `trackStaleness`. Those calls are at `scrape.go:1965` for cached series and `scrape.go:1986` for new ones. `trackStaleness` itself is at `scrape.go:1163-1165`.
- Series with an explicit exposition timestamp are skipped, unless `trackTimestampsStaleness` is set (`scrape.go:1964` and `1983-1984`).
- At the end of the append, `iterDone` swaps the maps and clears the new current map (`scrape.go:1102-1103`). `iterDone(true)` is called after a non-empty, error-free scrape (`scrape.go:1829-1835`).
- `updateStaleMarkers` runs when a scrape returns an empty body (`scrape.go:1781-1785`). That includes a failed scrape, because the scrape loop calls `append` anyway, with the comment "A failed scrape is the same as an empty scrape" (`scrape.go:1610-1613`). If the append itself fails, it retries with an empty body (`scrape.go:1623-1625`).
- `updateStaleMarkers` calls `forEachStale`, which yields every series in `seriesPrev` that is not in `seriesCur` (`scrape.go:1167-1175`). Each one is appended with `math.Float64frombits(value.StaleNaN)` at the scrape timestamp (`scrape.go:1753-1757`).
- Out-of-order and duplicate-timestamp errors are ignored here. The code comments that this is expected when a target goes away and comes back under a new scrape loop (`scrape.go:1759-1764`).
- I did not read the part of `append` between about lines 1841 and 1959. So I did not confirm exactly where the staleness check for a non-empty scrape is called. The diff of `seriesPrev` against `seriesCur` is clear from `iterDone` and `forEachStale`, but the non-empty case rests on that inference.

**2. A target stops being scraped**
- When the loop's `run` exits, a deferred function calls `endOfRunStaleness`. This only happens if the parent context is not cancelled and end-of-run markers have not been disabled (`scrape.go:1409-1419`).
- `endOfRunStaleness` (`scrape.go:1662-1728`) returns at once if there was never a scrape (`last.IsZero()`, line 1670). Otherwise it:
  - records `staleTime` at the next tick of the scrape-interval ticker, then waits one more tick and an extra `interval/10`, in case the target is recreated (lines 1675-1697);
  - bails out if the parent context is cancelled, which is treated as a server shutdown, or if markers were disabled while it waited (lines 1666-1668 and 1699-1702);
  - appends an empty scrape at `staleTime`, which reaches `updateStaleMarkers` and so marks every previously-seen series stale (lines 1704-1724);
  - calls `reportStale`, which writes stale markers for the synthetic report series `scrapeHealthMetric`, `scrapeDurationMetric`, `scrapeSamplesMetric` and `samplesPostRelabelMetric` (lines 1725 and 2330-2345). I read only the first four of these. The function may mark more series after line 2345.
- If the target comes back and is scraped first, the stale markers land out of order and are ignored (comment at `scrape.go:1705-1706`).
- When a loop is restarted and reuses its cache, `restartLoops` calls `oldLoop.disableEndOfRunStalenessMarkers()` (`scrape.go:363-365`). `disableEndOfRunStalenessMarkers` is defined at `scrape.go:1737-1739`. The same method is called per target from `scrapePool.disableEndOfRunStalenessMarkers` (`scrape.go:605-610`). I did not read where that pool-level method is invoked.

**Cache cleanup:** `iterDone(true)` deletes cache entries whose `lastIter` is not the current iteration (`scrape.go:1076-1084`).