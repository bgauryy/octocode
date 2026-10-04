The scrape job's `sample_limit` is enforced by a counting appender wrapper, `limitAppender`, that sits in front of the storage appender for each scrape. I did not trace what the scrape loop does with the batch after it returns the error. I also did not read the V2 scrape-loop code that uses `limitAppenderV2`.

- **Config:** `SampleLimit uint` (`yaml:"sample_limit"`) is a per-scrape-config field at `config/config.go:827`. It falls back to the global value at `config/config.go:938-939`. The scrape pool copies it into the loop as `sampleLimit: int(opts.sp.config.SampleLimit)` at `scrape/scrape.go:1349`.

- **Wrapping:** `appenderWithLimits` (`scrape/scrape.go:711`) adds `limitAppender{limit: sampleLimit}` only when `sampleLimit > 0` (`scrape/scrape.go:718-722`). The comment at line 717 says the limit applies after relabeling has dropped metrics. `scrapeLoop` builds this appender at `scrape/scrape.go:1827`.

- **Counting:** `limitAppender` is defined at `scrape/target.go:378`.
  - `Append` (`scrape/target.go:385-399`) and `AppendHistogram` (`scrape/target.go:401-415`) increment a counter `i` and return `errSampleLimit` once `i > limit`.
  - `errSampleLimit` is defined at `scrape/target.go:373` as "sample limit exceeded".
  - The count is skipped for a staleness marker on an already-known series (`ref != 0` and the value is a stale NaN). This keeps staleness markers writable for series already in the TSDB (`scrape/target.go:386-388`).
  - `limitAppenderV2` (`scrape/target.go:508` onward) does the same for the `AppenderV2` interface.

- **Handling in the scrape loop:**
  - `checkAddError` (`scrape/scrape.go:2159`) matches `errSampleLimit` (line 2163). It stores the error in `sampleLimitErr` and returns `false, nil`, so parsing continues and the total scraped count stays correct (comment at lines 2164-2165).
  - Once a limit error has occurred, `seriesAdded` is no longer incremented (`scrape/scrape.go:1988`).
  - After the loop, `sampleLimitErr` becomes the scrape error if none is set yet, and the `targetScrapeSampleLimit` metric is incremented once per scrape (`scrape/scrape.go:2055-2060`).
  - `Rollback()` is called in several places in `scrape/scrape.go` (for example lines 1511 and 1620), but I did not check which of them handles a sample-limit failure.

- **Reporting:** the configured limit is exposed as the `scrape_sample_limit` report sample (`scrape/scrape.go:2272` and `scrape/scrape.go:2320`).