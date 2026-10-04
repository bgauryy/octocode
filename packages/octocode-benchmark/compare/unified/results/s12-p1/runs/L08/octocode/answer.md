The sample limit is enforced inside the scrape loop by wrapping the storage appender with a counter. The counter rejects any sample over the limit, and the scrape is then failed after the whole body has been parsed. Everything below is from `scrape/`. I did not read the config parsing or the other `limitAppender` methods in full.

**Wiring**
- The job's `sample_limit` is copied into the loop at `scrape/scrape.go:1349`: `sampleLimit: int(opts.sp.config.SampleLimit)`.
- Each append pass calls `appenderWithLimits(sl.Appender, sl.sampleLimit, ...)` at `scrape/scrape.go:1827`.
- `appenderWithLimits` (`scrape/scrape.go:711-723`) wraps the appender in a `limitAppender` only when `sampleLimit > 0`. A limit of 0 means no limit.
- The wrap happens after relabeling, so samples dropped by relabeling don't count (comment at `scrape/scrape.go:717`).
- The V2 appender path does the same in `scrape/scrape_append_v2.go:37-49` and `:140`, using `limitAppenderV2`.

**The check**
- In `scrape/target.go`, `Append` increments `app.i` and returns `errSampleLimit` once `app.i > app.limit` (`scrape/target.go:390-391`). `errSampleLimit` is defined at `scrape/target.go:373` as "sample limit exceeded".
- The check is skipped for stale-NaN samples on series that already have a ref: the condition is `ref == 0 || !value.IsStaleNaN(v)`.
- The histogram path has the same check (`scrape/target.go:404-407`), and so does the V2 path (`scrape/target.go:519-522`).

**Handling the error**
- `checkAddError` (`scrape/scrape.go:2163-2167`) catches `errSampleLimit` and stores it in `sampleLimitErr`. It returns `false, nil`, so the loop keeps parsing. This is so the scraped-sample count stays correct, per the comment at `scrape/scrape.go:2164`.
- Series that hit the limit are not counted in `seriesAdded` (`scrape/scrape.go:1988`).
- After the loop, if `sampleLimitErr != nil`, it becomes the returned error and `targetScrapeSampleLimit` is incremented once per scrape (`scrape/scrape.go:2055-2060`). That counter is exported as `prometheus_target_scrapes_exceeded_sample_limit_total` (`scrape/metrics.go:214-216`).
- The returned error is handled by the deferred function at `scrape/scrape.go:1829`. I did not read that block, so I haven't confirmed that it rolls back the whole scrape's samples, though the metric's help text says over-limit scrapes are "rejected".

**Related**
- The configured limit is also reported as the `scrape_sample_limit` synthetic series (`scrape/scrape.go:2320`).