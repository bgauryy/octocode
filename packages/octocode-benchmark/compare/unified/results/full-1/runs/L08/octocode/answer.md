The sample limit is enforced per scrape by a wrapper appender (`limitAppender`) that counts appended samples and fails once the count passes the limit. All paths below are under `scrape/`. I didn't read where `sl.sampleLimit` is populated from the job config, so that link is unverified.

**Wrapping the appender**
- `appenderWithLimits` (`scrape.go:711`) builds the appender chain. It wraps the storage appender in a `limitAppender` only if `sampleLimit > 0` (`scrape.go:718-723`).
- A comment at `scrape.go:717` says the limit is applied after relabeling has dropped metrics, so dropped series don't count.
- The scrape loop calls it at `scrape.go:1827` with `sl.sampleLimit`.
- The V2 appender path has the same logic: `appenderV2WithLimits` (`scrape_append_v2.go:37`, `:44`).

**Counting and rejecting**
- `limitAppender` is defined in `target.go` with fields `limit` and `i`. `errSampleLimit` is `"sample limit exceeded"` (`target.go:373`).
- `Append` increments `app.i` and returns `0, errSampleLimit` when `app.i > app.limit` (`target.go:389-392`). `AppendHistogram` does the same (`target.go:405-408`).
- Staleness markers for series already in TSDB (`ref != 0`) skip the count, so they are always written (`target.go:385-387`).

**Handling the error in the scrape loop**
- `checkAddError` catches `errSampleLimit` (`scrape.go:2163`). It records the error in `*sampleLimitErr` and returns `false, nil`. Parsing continues so the total scraped sample count stays correct (`scrape.go:2164-2167`).
- After the loop, if `sampleLimitErr != nil` and no other error is set, it becomes the scrape's error (`scrape.go:2055-2058`).
- It also increments `targetScrapeSampleLimit` once per scrape (`scrape.go:2059-2060`).
- The configured limit is exposed as the `scrapeSampleLimitMetric` sample (`scrape.go:2320`).

**Not verified:** I didn't confirm whether the whole scrape's samples are discarded when the limit is hit. That depends on how `err` is used after the loop, which I didn't read.