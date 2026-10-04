The sample limit is enforced per scrape by an appender wrapper, `limitAppender`. It counts the samples appended in that scrape and returns `errSampleLimit` once the count passes the configured `sample_limit`. I did not trace what the caller does with the returned error after `append`, so the rollback behaviour is unverified.

- **Config to loop:** the job's `SampleLimit` is copied into the scrape loop as `sampleLimit: int(opts.sp.config.SampleLimit)` (`scrape/scrape.go:1349`, field at `:962`).
- **Wrapping:** `appenderWithLimits` wraps the storage appender in a `limitAppender` only when `sampleLimit > 0` (`scrape/scrape.go:711-723`). The comment at `:717` says the limit applies after relabeling has dropped metrics. `append` calls it at `scrape/scrape.go:1827`.
- **Counting:** `limitAppender.Append` and `AppendHistogram` (`scrape/target.go:385-415`) increment `app.i` and return `errSampleLimit` ("sample limit exceeded", defined at `:373`) when `app.i > app.limit`.
- **Staleness markers:** a stale-NaN marker for an already-known series (non-zero ref) bypasses the count, so markers are always written (`scrape/target.go:386-388` and `:402-404`).
- **Handling the error:**
  - `checkAddError` (`scrape/scrape.go:2163-2167`) stores the error in `sampleLimitErr` and keeps parsing. This keeps the total scraped-sample count correct.
  - While `sampleLimitErr` is set, `seriesAdded` is not incremented (`:1988`).
  - After the loop, the error becomes the scrape's returned error and `targetScrapeSampleLimit` is incremented once per scrape (`:2055-2060`).
  - The stale-marker update only runs if `err == nil` (`:2078-2079`).
- **Reported value:** the configured limit is exposed as the `scrape_sample_limit` report series (`scrape/scrape.go:2272`, `:2320`).
- **Appender v2:** `scrape/scrape_append_v2.go:45` and `scrape/target.go:508-522` have an equivalent `limitAppenderV2`.