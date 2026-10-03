1. **Helped:** The first `localSearch` call, with a regex for `sampleLimit|errSampleLimit|ErrSampleLimit` scoped to `scrape/` and excluding tests, gave nearly the whole picture in one shot. It showed the wrapper at `scrape.go:711`, the error at `target.go:373`, and the handling at `scrape.go:2163`. Two `localFetch` calls with explicit line ranges then confirmed the `limitAppender` body and the `checkAddError` branch.

2. **Did not help:**
   - Both first `localFetch` attempts failed validation. I passed `"365,415"` instead of `"365-415"`, and I repeated the mistake on the second call. The ranges format error message was clear, but I should have read the schema.
   - The multi-range fetch on `scrape.go` elided the middle lines. That was fine for my purposes, but it means I never saw the code after line 2065.
   - I didn't use `lspSearch` or `astSearch`.

3. **Next time:** Use `start-end` ranges from the start. Read the code after `scrape.go:2065` to see whether the batch is rolled back or committed. Check the V2 path properly instead of relying on grep hits.

4. **Confidence:** High on the core mechanism, because I read the code for `limitAppender`, `appenderWithLimits` and `checkAddError`. Medium on end-of-scrape consequences, since I didn't read them.