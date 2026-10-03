A scrape job's `sample_limit` is enforced by a wrapper appender that counts samples as the scrape loop appends them. Once the count passes the limit, appends fail with `errSampleLimit`, and the scrape is reported as failed. I read this from the local checkout and did not run it.

**Config**
- `SampleLimit uint` (`sample_limit`) is defined at `config/config.go:510` on the scrape config, with a global default at `config/config.go:827`.
- The comment there says that more than this many samples after metric relabeling will fail the scrape, and that 0 means no limit.
- A job that sets 0 inherits the global value (`config/config.go:938-939`).
- `scrape/scrape.go:1349` copies it into the scrape loop as `sampleLimit: int(opts.sp.config.SampleLimit)`.

**Wrapping the appender**
- Each scrape (`scrape/scrape.go:1827`) calls `appenderWithLimits(sl.Appender, sl.sampleLimit, sl.bucketLimit, sl.maxSchema)`.
- Inside it, `scrape/scrape.go:717-721` wraps the appender in a `&limitAppender{limit: sampleLimit}` only when `sampleLimit > 0`.
- A comment at line 717 says the limit is applied after metrics are potentially dropped via relabeling.

**Counting** (`scrape/target.go:378-415`)
- `limitAppender` holds `limit` and a counter `i`.
- `Append` (`scrape/target.go:385-399`) and `AppendHistogram` (`scrape/target.go:401-415`) increment `i` and return `0, errSampleLimit` once `i > limit` (lines 389-392 and 405-408).
- `errSampleLimit` is defined at `scrape/target.go:373-374` as "sample limit exceeded".
- Staleness markers for series that already exist (ref != 0) skip the count, so those markers are still written (`scrape/target.go:386-388` and `402-404`).
- There is also a `limitAppenderV2` at `scrape/target.go:508-509`. I did not read its body.

**Handling the error** (`scrape/scrape.go`)
- `checkAddError` (line 2159) matches `errors.Is(err, errSampleLimit)` at line 2163 and stores it in `*sampleLimitErr`.
- It returns `false, nil`, so parsing continues and the total number of samples scraped is still reported. The over-limit sample is dropped. The code at line 1988 skips the `seriesAdded++` increment while `sampleLimitErr` is set.
- After the loop (lines 2055-2061), the scrape's returned error is set to `sampleLimitErr` if nothing else failed. `sl.metrics.targetScrapeSampleLimit` is incremented once per scrape.

**Not verified:** I did not read how the returned error becomes the target's scrape failure, nor whether the already-appended samples are rolled back.