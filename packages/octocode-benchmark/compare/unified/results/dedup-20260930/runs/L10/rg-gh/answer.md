**Answer:** `rate()` and `increase()` share one implementation, `extrapolatedRate` in `promql/functions.go:452`. The only difference is a flag: `rate` calls it with `isCounter=true, isRate=true` (`funcRate`, `promql/functions.go:811-813`). `increase` calls it with `isCounter=true, isRate=false` (`funcIncrease`, `promql/functions.go:816-818`).

**Algorithm for float counters**

1. **Raw delta.** The result starts as last sample minus first sample in the window (`promql/functions.go:510`).
2. **Counter resets.** For each consecutive pair of samples, if the current value is lower than the previous one, the previous value is added to the result (`:523-528`). The same reset rule applies when a start-timestamp (ST) reset is detected. Histograms take the equivalent path through `histogramRate` (`:640`), which I did not read.
3. **Boundary distances.** The code computes `durationToStart` and `durationToEnd`, the gaps between the first/last sample and the window edges. The window is `[enh.Ts - Range - Offset, enh.Ts - Offset]` (`:469-470`, `:535-536`).
4. **Extrapolation threshold.** The threshold is 1.1 × the average gap between samples (`:543`).
   - A boundary gap at or above the threshold is replaced by half the average gap (`:581-583` for the start, `:602-604` for the end).
   - A smaller gap is extrapolated all the way to the edge.
5. **Zero clamp (counters only).** If the result is positive and the first sample is ≥ 0, the code estimates when the counter would have been zero: `sampledInterval * first/result`. If that is shorter than `durationToStart`, it becomes the start distance, so the extrapolation never goes below zero (`:585-601`).
6. **Scaling factor.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or 1 if `sampledInterval` is 0 (`:606-609`).
   - For `rate`, `factor` is also divided by the range in seconds (`:610-612`).
   - The result is `resultFloat *= factor` (`:614-616`).

For `increase`, the output is the extrapolated total over the window. For `rate`, it is that total divided by the range duration. It is not divided by the sampled interval.

**Edge cases**
- **Too few samples.** With fewer than two samples, nothing is returned, unless the ST shortcut below applies (`:546-562`, `:577-579`).
- **Start-timestamp shortcut.** If the first sample's ST lies inside the window, the code treats the counter as starting at zero at that ST. It sets `durationToStart=0`, uses `lastT - ST` as the interval, and adds the first sample's value to the result (`:546-566`). A single sample works in this case.
- **Mixed samples.** A window with both float and histogram samples yields a warning and no result (`:487-489`).
- **Smoothed and anchored selectors.** These skip this path and use `extendedRate` or `extendedHistogramRate` (`:453-469`). I did not read those functions.

**Uncertainty:** The line numbers above are approximate. I read the file in one pass and did not number the lines. The function start lines (`:452`, `:640`, `:811`, `:816`) come from an `rg` search and are exact.