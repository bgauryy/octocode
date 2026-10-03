Prometheus marks a series stale by appending a special NaN sample, `value.StaleNaN`. It does this in two ways: by diffing the series seen in this scrape against the previous scrape, and by an end-of-run pass when a scrape loop stops. I didn't read `scrapeAndReport` (around lines 1560–1640), so I haven't confirmed that a failed scrape reaches the empty-append path. All lines below are in `scrape/scrape.go` unless noted.

**The marker**
- `model/value/value.go:28` defines `StaleNaN uint64 = 0x7ff0000000000002`, a signaling NaN.
- `IsStaleNaN` (`model/value/value.go:32-33`) tests for that exact bit pattern.

**1. A series disappears from a scrape**
- `scrapeCache` tracks two sets of series. Each series seen in the current scrape is recorded with `trackStaleness`, which does `c.seriesCur[ref] = ce`. It is called at about lines 1966 and 1987, once per series appended.
- `forEachStale` iterates `seriesPrev` and calls back for every ref that is not in `seriesCur`. That is the set of series that disappeared.
- `updateStaleMarkers` (about line 1754) calls `forEachStale`. For each vanished series it appends `math.Float64frombits(value.StaleNaN)` at the scrape timestamp `defTime`.
- It ignores `ErrOutOfOrderSample` and `ErrDuplicateSampleForTimestamp`. The comment says these are expected when a target goes away and returns with a new scrape loop.
- It is called after a successful non-empty append (about line 2082, only `if err == nil`). It is also called at line ~1784, the `len(b) == 0` case, which handles an empty scrape body. There it calls `sl.cache.iterDone(false)`, which swaps the cache without flushing it. A non-empty scrape ends with `iterDone(true)`, at about line 1835.
- Series that are dropped by relabeling are deliberately not tracked, so they get no stale markers (comment near line 1980).

**2. A target stops being scraped**
- The loop's deferred function runs when it exits (about lines 1409–1420). If `sl.parentCtx.Err() == nil` and end-of-run markers are not disabled, it calls `sl.endOfRunStaleness(last, ticker, sl.interval)`.
- `endOfRunStaleness` (about line 1662) works in this order:
  - It returns immediately if there was never a scrape (`last.IsZero()`).
  - It returns if the parent context is cancelled. That is treated as a server shutdown, so no markers are written.
  - It waits for one ticker tick and records `staleTime`.
  - It waits a second tick, so a recreated target has time to ingest samples.
  - It waits another `interval/10`.
  - It re-checks `disabledEndOfRunStalenessMarkers`.
  - It then runs an empty-scrape append, which reaches the `len(b)==0` branch and `updateStaleMarkers`. Every series in the previous scrape gets a stale marker. If the target was recreated and has already scraped, the markers are out of order and dropped.
- `reportStale` (about line 2331) also writes `StaleNaN` for the synthetic report series: `up` (`scrapeHealthMetric`), the scrape duration, samples and post-relabel sample metrics, and `scrapeSeriesAddedMetric`. With `reportExtraMetrics` set, it also does the timeout, sample-limit and body-size metrics. A call to `reportStale(app, staleTime)` sits in the same function at about line 1725.

**Exception: reloads**
- `scrapePool.restartLoops` (line ~364) calls `oldLoop.disableEndOfRunStalenessMarkers()` when `reuseCache` is true. The new loop inherits the cache, so no markers are written for series that are still present.
- `scrapePool.disableEndOfRunStalenessMarkers(targets)` (line ~605) does the same for a given set of targets.

**Uncertainty**
- I didn't confirm every caller of `scrapePool.disableEndOfRunStalenessMarkers(targets)`.
- Line numbers marked "about" come from the outline output of a match search. I saw the code text there but didn't read those lines directly.