The sample limit is enforced by a `limitAppender` that wraps the scrape loop's appender. It counts samples per scrape and returns `errSampleLimit` once the count passes the limit. The scrape loop then fails the whole scrape. I read the code but did not run it, and there is also a V2 appender path that I only skimmed.

**Config**
- `sample_limit` is a `uint` on the scrape config (`config/config.go:827`), with a global default at `config/config.go:510`.
- A job with no value inherits the global one (`config/config.go:938-939`).
- It reaches the scrape loop as `sampleLimit: int(opts.sp.config.SampleLimit)` (`scrape/scrape.go:1349`).

**Wiring**
- Each scrape calls `appenderWithLimits(sl.Appender, sl.sampleLimit, sl.bucketLimit, sl.maxSchema)` (`scrape/scrape.go:1827`).
- That function wraps the appender in `limitAppender` only when `sampleLimit > 0` (`scrape/scrape.go:711-721`). A limit of 0 means no limit.
- A comment there says the limit is applied after relabeling has dropped metrics, so dropped series don't count.

**Counting** (`scrape/target.go:377-415`)
- `limitAppender` keeps a counter `i`.
- `Append` and `AppendHistogram` increment it, and return `errSampleLimit` ("sample limit exceeded", defined at `scrape/target.go:373`) once `i > limit`.
- Staleness markers for series already in TSDB (non-zero `ref`) skip the count, so they are always written (`scrape/target.go:386-393`).
- `limitAppenderV2` (`scrape/target.go:509-526`) applies the same logic for the `AppenderV2` interface.

**Handling the error** (`scrape/scrape.go`)
- `checkAddError` catches `errSampleLimit`, stores it in `sampleLimitErr`, and returns `sampleAdded=false` with no error. Parsing continues so the total scraped sample count is still reported (`scrape/scrape.go:2163-2167`).
- While `sampleLimitErr` is set, `seriesAdded` is not incremented (`scrape/scrape.go:1988`).
- After the loop, `sampleLimitErr` becomes the scrape's returned error. The `targetScrapeSampleLimit` metric is incremented once per scrape (`scrape/scrape.go:2055-2060`).
- The configured limit is also exported as the `scrape_sample_limit` report metric (`scrape/scrape.go:2272`, `scrape/scrape.go:2320`).

**Not checked:** I did not read the code that acts on the returned error, so I can't say from this what happens to the samples already appended. I also did not check which of the V1 and V2 appender paths is active at this commit. I couldn't run `git` here, so I didn't confirm the checkout is at `ea954809ce`. The line numbers come from the local files as they are.