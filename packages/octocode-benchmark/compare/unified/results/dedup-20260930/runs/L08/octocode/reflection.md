**1. Helped:**
- The single `localSearch` for `errSampleLimit|sampleLimit|SampleLimit` over `scrape/` returned line anchors in `scrape.go`, `scrape_append_v2.go` and `target.go`. That located the whole mechanism in one call.
- The batched `localFetch` with four line ranges then confirmed `limitAppender`, `appenderWithLimits` and `checkAddError` in one round trip.

**2. Did not help:**
- My first `localSearch` failed validation because `exclude` was passed as a bare string instead of an array. That cost one wasted call.
- I never read the second page of matches (20 total, only 10 shown per file list).
- I did not look at the rollback or commit code after the error. I also did not check the config-side plumbing that sets `sampleLimit` per job. Both are gaps in the answer, and I flagged them as unverified.
- Line citations came from `localFetch` ranges I chose myself. I did not re-verify the exact struct line numbers (I cited 376-381 for `limitAppender`), so those are approximate.

**3. Next time:**
- Pass array parameters correctly on the first try.
- Add one more `localSearch` on `Rollback` or `Commit` near the `sampleLimitErr` handling, to settle what happens to the partial samples.
- Add one search on `SampleLimit` in `config/` to trace the job config to `sampleLimit`.

**4. Confidence:** High on the core mechanism (`limitAppender` counting, the `errSampleLimit` return, the staleness exemption, and `checkAddError` handling), because I read that source directly. Medium on the completeness of the full flow, for the gaps above.