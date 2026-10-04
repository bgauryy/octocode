`rate()` and `increase()` are both thin wrappers around one function, `extrapolatedRate`, in `promql/functions.go`. I read the code but did not run it. I did not read `extendedRate` or `extendedHistogramRate`, the separate paths for smoothed and anchored selectors.

- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`functions.go:811-813`).
- `funcIncrease` calls it with `isCounter=true, isRate=false` (`functions.go:816-818`).
- `delta` uses the same function with `isCounter=false` (`functions.go:806-808`).

**Float counters, step by step (`functions.go:452-633`):**
1. **Raw increase.** It takes the last sample value minus the first (`:512`).
2. **Counter resets.** It walks consecutive pairs. Whenever `curr.F < prev.F`, it adds `prev.F` to the result (`:521-534`). The same correction applies when the start-timestamp (ST) logic detects a reset via `isStartTimestampReset` (`:531`, defined at `:760`).
3. **Minimum samples.** Without a usable ST, fewer than two samples return nothing (`:572-574`).
4. **Boundary gaps.** It measures the gap from the window start to the first sample and from the last sample to the window end (`:541-542`). The window runs from `enh.Ts - (Range + Offset)` to `enh.Ts - Offset` (`:474-475`).
5. **Extrapolation threshold.** The threshold is 1.1 × the average spacing between samples (`:544-549`).
6. **Extrapolating each end.**
   - A gap of at least the threshold is replaced by half the average spacing (`:588-590` for the start, `:615-617` for the end).
   - A gap below the threshold is extrapolated all the way to the window edge.
7. **No negative values at the start.** For counters, the start extrapolation is capped at the time the counter would have been zero. That time is `sampledInterval * (firstValue / increase)`, and it applies only if the increase is positive and the first value is non-negative (`:591-612`).
8. **Scaling.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval` (`:619-622`). The result is multiplied by this factor (`:626-627`).
9. **Rate versus increase.** For `rate`, the factor is also divided by `ms.Range.Seconds()` (`:623-625`). So `rate` is the extrapolated increase divided by the window length in seconds, not by the sampled interval. `increase` is the extrapolated increase itself.

**Start-timestamp case (`:551-571`):** If the first sample's ST is non-zero, inside the window, and before the first sample, the code treats the counter as starting from zero at the ST. It sets `durationToStart = 0`, adds the first sample's value to the result, and measures the sampled interval from the ST. A single sample can then produce a result.

**Native histograms:** The same function handles them. `histogramRate` (`:640-757`) subtracts the first histogram from the last. It adds `prev` back at each reset, using `DetectReset` or the ST check, and reconciles bucket schemas to the smallest one. The final histogram is then scaled by the same factor (`:629`).

**Other behaviour:**
- Windows that mix float and histogram samples are dropped with a warning (`:484-486`).
- Smoothed or anchored selectors take a separate path through `extendedRate` or `extendedHistogramRate` (`:455-469`).