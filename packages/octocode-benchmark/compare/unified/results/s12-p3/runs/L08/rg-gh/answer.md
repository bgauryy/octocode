**Short answer:** `sample_limit` is enforced per scrape, by an appender wrapper. A scrape that exceeds the limit is flagged as failed, and the over-limit samples are not appended. I read the source at the checkout but didn't confirm it is exactly `ea954809ce` (git failed in the sandbox). I also didn't run any tests.

**1. Config.** `ScrapeConfig.SampleLimit` (`sample_limit`) is defined at `config/config.go:827`. It falls back to the global value at `config/config.go:938-939`. At `scrape/scrape.go:1349` the scrape loop takes it as `sampleLimit: int(opts.sp.config.SampleLimit)`.

**2. Wrapper.** There are two appender paths, and both apply the limit only when it is above 0.
- `appenderWithLimits` (`scrape/scrape.go:711-722`) wraps the appender in a `limitAppender`. The comment says the limit applies after relabeling drops.
- `appenderV2WithLimits` (`scrape/scrape_append_v2.go:37-47`) does the same with `limitAppenderV2`.
- The scrape loop builds the wrapper at `scrape/scrape.go:1827` (v1) and `scrape/scrape_append_v2.go:140` (v2).

**3. Counting.**
- `limitAppender` is defined at `scrape/target.go:376-380`. Its `Append` and `AppendHistogram` methods are at `scrape/target.go:382-415`. `limitAppenderV2.Append` is around `scrape/target.go:516-524`.
- Each call increments `app.i`. Once `app.i > app.limit`, it returns `errSampleLimit` ("sample limit exceeded", `scrape/target.go:373`) without calling the underlying appender.
- Staleness markers for series that already exist (non-zero ref) bypass the count, so those markers are always written (`scrape/target.go:383-384`).

**4. Handling the error.**
- `checkAddError` (`scrape/scrape.go:2163-2167`) catches `errSampleLimit`. It records it in `sampleLimitErr` and returns without failing, so parsing continues. This keeps the total scraped-sample count correct.
- After the loop (`scrape/scrape.go:2055-2060`), the scrape returns `sampleLimitErr` as its error. It also increments the `targetScrapeSampleLimit` counter once per scrape. That counter is defined in `scrape/metrics.go:214`.
- The v2 path does the same at `scrape/scrape_append_v2.go:374-379`.

**5. Reporting.** The configured limit is exposed as the `scrape_sample_limit` report series, written at `scrape/scrape.go:2320`.

**Uncertainty:** I didn't read what happens to the batch after the error is returned. That is, I didn't check whether the already-appended samples are rolled back or committed. Look at the code after line 2060 in `scrape/scrape.go` if you need that.