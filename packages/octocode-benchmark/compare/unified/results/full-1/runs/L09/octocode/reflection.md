1. **Helped:** The first successful `localSearch` for `StaleNaN` with `include`/`exclude` globs gave a quick map of the codebase. It pointed straight at `scrape/scrape.go`. The `localFetch` line-range reads of `updateStaleMarkers`, `endOfRunStaleness` and `forEachStale` gave the core evidence. The `matchString` regex read showed every call site in a single outline.

2. **Did not help:**
- My first `localSearch` failed because I passed `include`/`exclude` as strings, not arrays.
- My first `localFetch` batch failed because it lacked the required `goal`/`reasoning`.
- A `matchString` regex with an unbalanced `(` errored.
- The `matchString` output showed line numbers that did not line up with my requested ranges. My cited line numbers ("about 1754", "about 2082") are therefore approximate.
- I never read `scrapeAndReport`, so I couldn't confirm that a failed scrape reaches the empty-append path. I flagged that in the answer.
- I also never checked all callers of `scrapePool.disableEndOfRunStalenessMarkers`.

3. **Next time:** Read schemas before the first call. Use `localSearch` with simple literals and a few lines of context, so I get exact line numbers instead of outline-style output. Read `scrapeAndReport` directly.

4. **Confidence:** Medium-high on the mechanism, because I read the code. Medium on exact line numbers and the failed-scrape path.