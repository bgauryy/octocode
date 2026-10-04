`rate()` and `increase()` are thin wrappers around one shared function, `extrapolatedRate`. `rate` is `increase` divided by the range length in seconds. All lines below are in `promql/functions.go` at ea954809ce.

**Entry points**
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`:811-813`).
- `funcIncrease` calls it with `isCounter=true, isRate=false` (`:816-818`).
- `funcDelta` calls it with `false, false` (`:806-808`).

**Computation for float counters** (`extrapolatedRate`, `:452-633`)
1. **Raw increase.** It starts from the last sample minus the first sample, `resultFloat = last.F - first.F` (`:512`).
2. **Counter resets.** It walks the samples pairwise. Whenever `currPoint.F < prevPoint.F`, it adds `prevPoint.F` to the result (`:521-534`). The same correction applies when a start-timestamp reset is detected via `isStartTimestampReset`. If start timestamps overlap, it adds a warning annotation (`:524-530`).
3. **Boundary distances and threshold.**
   - `durationToStart` is the first sample time minus `rangeStart`, and `durationToEnd` is `rangeEnd` minus the last sample time (`:541-542`). `rangeStart` and `rangeEnd` account for the offset (`:474-475`).
   - The average gap between samples is `sampledInterval / (n-1)`, and the extrapolation threshold is that average × 1.1 (`:544-549`).
4. **Extrapolation.**
   - If `durationToStart >= threshold`, it is cut to half the average gap (`:588-590`). The same rule applies to `durationToEnd` (`:615-617`).
   - For counters, the start extrapolation is also capped at the zero point, `sampledInterval * (first.F / resultFloat)`, so the counter is never extrapolated below zero (`:591-612`). This only applies when `resultFloat > 0` and `first.F >= 0` (`:599-602`).
5. **Scaling.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval` (`:619-622`).
   - For `rate`, it also divides by `ms.Range.Seconds()` (`:623-625`). This is the length of the selector's range, not the sampled interval.
   - The result is `resultFloat *= factor` (`:626-627`).

**Other cases**
- **Histograms.** Native histograms go through the same flow. `histogramRate` computes the increase (`:500`), and the result is scaled with `resultHistogram.Mul(factor)` (`:629`). The zero-point cap uses the histogram `Count`. A window with both floats and histograms is dropped with a mixed-type warning (`:484-486`).
- **Start timestamps.**
  - If the first sample's start timestamp lies inside the range and before the first sample, the code assumes a zero-valued sample at that start timestamp (`:551-571`). It sets `durationToStart = 0`, adds the first sample's value to the result, and uses `lastT - sts[0]` as the sampled interval.
  - With a valid start timestamp, a single sample can still produce a result. Without one, a single sample returns nothing (`:572-574`), and fewer than two samples generally produces no output.
- **Anchored or smoothed selectors.** These are handled by `extendedRate` or `extendedHistogramRate` instead (`:455-466`). I did not read those two functions.

I did not read `histogramRate` (`:640`) or the start-timestamp helper functions.