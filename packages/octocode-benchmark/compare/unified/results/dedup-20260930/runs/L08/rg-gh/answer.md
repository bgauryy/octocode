The sample limit is enforced per scrape by a wrapping appender that counts appends and returns `errSampleLimit` once the count passes the limit. The scrape loop then fails the whole scrape.

**1. Config to loop.** `scrapeLoop.sampleLimit` is set from `opts.sp.config.SampleLimit` (`scrape/scrape.go:1349`).

**2. Appender wrapping.**
- At the start of each scrape append, `scrape/scrape.go:1827` calls `appenderWithLimits(sl.Appender, sl.sampleLimit, ...)`.
- `appenderWithLimits` (`scrape/scrape.go:711`) wraps the appender in a `limitAppender` only when `sampleLimit > 0` (`scrape/scrape.go:718-722`). A limit of 0 means no limit.
- The code comment at `scrape/scrape.go:717` says the limit applies after relabeling has dropped metrics.
- The V2 appender path works the same way. `appenderV2WithLimits` is at `scrape/scrape_append_v2.go:37`, and the call is at `scrape/scrape_append_v2.go:140`.

**3. The counter.**
- `limitAppender` is in `scrape/target.go:378-`. Its `Append` and `AppendHistogram` methods increment `app.i` and return `errSampleLimit` ("sample limit exceeded", `scrape/target.go:373`) when `app.i > app.limit` (`scrape/target.go:388-392`, `scrape/target.go:403-408`).
- The V2 equivalent, `limitAppenderV2.Append`, returns the same error (`scrape/target.go:522`).
- Staleness markers for already-known series bypass the count. The check is `ref == 0 || !value.IsStaleNaN(v)`, so the markers are always written (`scrape/target.go:383-385`).

**4. Scrape loop handling.**
- `checkAddError` catches `errSampleLimit`. It records it in `sampleLimitErr`, returns `sampleAdded=false` and keeps parsing, so the total scraped-sample count stays correct (`scrape/scrape.go:2163-2166`).
- After the parse loop, the loop makes `sampleLimitErr` the scrape's error and increments `targetScrapeSampleLimit` once per scrape (`scrape/scrape.go:2055-2060`). The V2 version is at `scrape/scrape_append_v2.go:374-379`.
- The `sampleLimitErr` check at `scrape/scrape.go:1988` (I did not read the surrounding code) appears to be what gates the normal success path.
- The limit itself is exported as a report metric, `scrapeSampleLimitMetric` (`scrape/scrape.go:2272`, `scrape/scrape.go:2320`).

**Not verified.** I did not read the code after `scrape/scrape.go:1988`, so I can't say exactly what happens to the samples already appended when the limit is hit. `TestScrapeLoopRunCreatesStaleMarkersOnSampleLimit` (`scrape/scrape_test.go:3027`) looks like the place that covers that behaviour.