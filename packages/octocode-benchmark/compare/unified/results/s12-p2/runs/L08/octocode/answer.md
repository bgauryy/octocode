The sample limit is enforced by a wrapper appender, `limitAppender`. It counts appended samples during each scrape and returns `errSampleLimit` once the count passes the limit. The scrape loop then fails the whole scrape with that error. I read the v1 appender path in `scrape/scrape.go` and `scrape/target.go`. I only grepped the v2 path in `scrape/scrape_append_v2.go` and did not read its code.

1. **Config to loop:** the job's `SampleLimit` is copied into the scrape loop as `sampleLimit: int(opts.sp.config.SampleLimit)` (`scrape/scrape.go:1349`).

2. **Wrapping the appender:**
   - Each scrape calls `appenderWithLimits(sl.Appender, sl.sampleLimit, ...)` (`scrape/scrape.go:1827`).
   - If `sampleLimit > 0`, that function wraps the appender in a `limitAppender` (`scrape/scrape.go:718-723`).
   - A comment says the limit is applied after relabeling has dropped metrics (`scrape/scrape.go:717`).
   - The wrapper sits on top of a `timeLimitAppender`, and the bucket-limit and max-schema wrappers go on top of it (`scrape/scrape.go:712-735`).
   - The v2 path has an equivalent `appenderV2WithLimits` (`scrape/scrape_append_v2.go:37-47`).

3. **The check itself** (`limitAppender`, `scrape/target.go:377-415`):
   - `Append` and `AppendHistogram` increment a per-batch counter `i`. If `i > limit`, they return `errSampleLimit` ("sample limit exceeded", `scrape/target.go:373`) without writing the sample (lines 388-392 and 404-408).
   - Staleness markers for series that already exist (`ref != 0`) skip the counter, so those markers are always written (comments at `scrape/target.go:386-387` and `402-403`).

4. **Handling the error in the scrape loop:**
   - `checkAddError` catches `errSampleLimit`, stores it in `sampleLimitErr`, and returns `false, nil` (`scrape/scrape.go:2163-2167`). Parsing continues so the total number of scraped samples is still reported.
   - After the loop, if `sampleLimitErr != nil`, it becomes the scrape error when no other error is set. The `targetScrapeSampleLimit` counter is incremented once per scrape (`scrape/scrape.go:2055-2061`).
   - I did not read the code between lines 2066 and 2158. That is where the commit-or-rollback handling would be, so I can't confirm from my reading that the batch is rolled back.

5. **Reporting:** the configured limit is exposed as a report sample through `scrapeSampleLimitMetric` (`scrape/scrape.go:2320`).