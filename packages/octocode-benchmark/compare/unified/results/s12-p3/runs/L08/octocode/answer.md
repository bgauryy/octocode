The sample limit is enforced in the scrape loop's appender. `limitAppender` counts samples appended in one scrape and returns `errSampleLimit` once the count passes the limit. The scrape loop then keeps parsing, and at the end it fails the whole scrape with that error.

1. **Config to loop.** The job's `SampleLimit` is copied into the scrape loop as `sampleLimit: int(opts.sp.config.SampleLimit)` at `scrape/scrape.go:1349`. The field is declared at `scrape/scrape.go:962`.

2. **Appender wrapping.**
   - Each scrape builds its appender with `appenderWithLimits(sl.Appender, sl.sampleLimit, …)` (`scrape/scrape.go:1827`). The V2 path does the same at `scrape/scrape_append_v2.go:140`.
   - `appenderWithLimits` (`scrape/scrape.go:711-723`) wraps the appender in a `limitAppender` only if `sampleLimit > 0`. A limit of 0 means no limit.
   - The comment at line 717 says the limit is applied after relabeling has dropped metrics.
   - The V2 equivalent is `appenderV2WithLimits` at `scrape/scrape_append_v2.go:37-47`.

3. **Counting.** `limitAppender` (`scrape/target.go:378-415`) increments `app.i` on each `Append` and `AppendHistogram`. When `app.i > app.limit` it returns `errSampleLimit` ("sample limit exceeded", defined at `scrape/target.go:373`) at lines 390-391 and 406-407.
   - Staleness markers for series already in TSDB (non-zero ref) are not counted. This ensures the marker is always written (comments at lines 386-387 and 402-403).

4. **Handling the error.** `checkAddError` (`scrape/scrape.go:2159-2167`) matches `errSampleLimit` and stores it in `sampleLimitErr`. It returns `sampleAdded=false` with a nil error, so parsing continues. The comment says this is so the total number of samples scraped is still reported correctly.
   - Series that hit the limit aren't added to the series count (`scrape/scrape.go:1988`).

5. **End of scrape.** If `sampleLimitErr != nil` (`scrape/scrape.go:2055-2061`), it becomes the returned error when none is already set. The `targetScrapeSampleLimit` counter is incremented once per scrape, outside the loop. The V2 path does the same at `scrape/scrape_append_v2.go:374-379`.
   - I did not read the code between lines 2067 and 2158 or the caller that handles the returned error. So I can't say here exactly what happens to the already-appended samples, such as whether the appender is rolled back. The sample-limit value is also exposed as a report metric (`scrape/scrape.go:2320`).