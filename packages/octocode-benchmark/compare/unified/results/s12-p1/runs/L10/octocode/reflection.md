1. **Helped:** The first `localSearch` was the fastest step. One regex alternation (`func extrapolatedRate|func funcRate|func funcIncrease|func instantValue`) returned all four declarations with line numbers in a single call. The two parallel `localFetch` line-range reads then covered everything: lines 440-640 for the full body and 805-825 for the wrappers. The numbered output let me cite `path:line` directly.

2. **Did not help:** Nothing failed, and I made no redundant calls. Limits:
   - I didn't read `extendedRate`, `extendedHistogramRate`, `histogramRate`, `isStartTimestampReset` or `checkStartTimeOverlap`, so I only described those paths from their call sites.
   - I didn't check how `rangeStart` and `rangeEnd` or the sample window are built in the engine (`engine.go`). That boundary behavior is inferred from `extrapolatedRate` alone.
   - I didn't check commit history or tests.

3. **Next time:** Do the same search and read, then add one more `localFetch` of `extendedRate` and one of the start-timestamp helpers. That would close the gaps. I'd also look at the engine's matrix-selector range handling to confirm range exclusivity.

4. **Confidence:** High for the core algorithm (reset handling, 1.1× threshold, zero-point cap, factor, division by range seconds), because I read that code directly. Medium for the start-timestamp and histogram details, because I read only the main function's use of the helpers and not the helpers themselves.