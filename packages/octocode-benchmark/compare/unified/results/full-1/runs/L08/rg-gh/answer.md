A scrape job's `sample_limit` is enforced by an appender wrapper, `limitAppender`. It counts samples appended in one scrape and returns `errSampleLimit` once the count passes the limit. The scrape loop then turns that into a failed scrape. I read the code but did not run it or check the tests.

**Where the limit comes from**
- `scrape/scrape.go:1349` copies `config.SampleLimit` from the job's scrape config into `scrapeLoop.sampleLimit` (field at `scrape/scrape.go:962`).

**How the wrapper is installed**
- At `scrape/scrape.go:1827` each scrape's append pass builds its appender with `appenderWithLimits(...)`.
- `appenderWithLimits` (`scrape/scrape.go:711-722`) wraps the appender in a `limitAppender` only when `sampleLimit > 0`. A limit of 0 means no limit.
- The code comment says the limit is applied after relabeling has dropped metrics (`scrape/scrape.go:717`).
- The newer `AppenderV2` path does the same through `appenderV2WithLimits` (`scrape/scrape_append_v2.go:37-45`, called at `:140`) and `limitAppenderV2` (`scrape/target.go:508-525`).

**How the count is enforced**
- `errSampleLimit` is defined at `scrape/target.go:373`.
- `limitAppender.Append` and `AppendHistogram` (`scrape/target.go:385-411`) increment a counter `i` on each append. When `i > limit` they return `errSampleLimit` and do not write the sample.
- Stale markers for series already in storage (non-zero `ref`) skip the count. This means the marker is still written when a series disappears (`scrape/target.go:386-387`, `:402-403`).

**How the scrape loop reacts**
- `checkAddError` (`scrape/scrape.go:2163-2167`) catches `errSampleLimit`. It records the error in `sampleLimitErr` and keeps parsing, so the total sample count stays correct.
- Samples over the limit are not counted as added series (`scrape/scrape.go:1988`).
- After the loop (`scrape/scrape.go:2055-2060`), a non-nil `sampleLimitErr` becomes the scrape's error if none is set yet. It also increments the `targetScrapeSampleLimit` metric once per scrape.
- The configured limit is exposed as a report sample, `scrapeSampleLimitMetric` (`scrape/scrape.go:2272`, written at `:2320`).

**Uncertainty:** I did not trace how the scrape error changes the `up` value or what happens to the samples already appended before the limit was hit.