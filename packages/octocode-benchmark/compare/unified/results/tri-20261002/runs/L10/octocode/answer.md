`rate()` and `increase()` are both thin wrappers around one function, `extrapolatedRate` in `promql/functions.go:452`. I read the float-sample path and the entry points. I only skimmed `histogramRate` and did not read `extendedRate` or `extendedHistogramRate`.

**Entry points**
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`promql/functions.go:811-813`).
- `funcIncrease` calls it with `isCounter=true, isRate=false` (`promql/functions.go:816-818`).
- `delta` uses the same function with both flags false (`promql/functions.go:806-808`).

**Algorithm for float counters (default path)**
1. **Raw increase.** `resultFloat = last.F - first.F` (`promql/functions.go:512`).
2. **Counter resets.** The code walks consecutive pairs. If `curr.F < prev.F`, it adds `prev.F` to the result (`promql/functions.go:521-534`). It also treats a start-timestamp reset as a reset, via `isStartTimestampReset` on the same line range.
3. **Window and spacing.** The window runs from `rangeStart = enh.Ts - (Range + Offset)` to `rangeEnd = enh.Ts - Offset` (`promql/functions.go:474-475`). It computes these from the window edges and sample times:
   - `durationToStart` and `durationToEnd`, the gaps between the first/last sample and the window edges.
   - `sampledInterval = lastT - firstT`.
   - `averageDurationBetweenSamples = sampledInterval / (n-1)`.
   - `extrapolationThreshold = 1.1 × average` (`promql/functions.go:541-549`).
4. **Edge extrapolation.** If a gap to the start or end is at least the threshold, it is replaced by `average/2` (`promql/functions.go:588-590` and `615-617`).
5. **Zero clamp for counters.** If the result is positive and the first value is non-negative, the code computes `durationToZero = sampledInterval * (first.F / resultFloat)`. If that is shorter than `durationToStart`, it becomes the new `durationToStart`. This stops the extrapolation from going below zero (`promql/functions.go:591-612`).
6. **Scaling factor.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval` (`promql/functions.go:619-622`).
   - For `rate`, it divides the factor by `ms.Range.Seconds()` (`promql/functions.go:623-625`).
   - It then multiplies the result by the factor (`promql/functions.go:626-630`).
   - So `increase` is the extrapolated total over the window, and `rate` is that total divided by the range in seconds.

**Edge cases**
- **Too few samples.** Normally at least two samples are needed. With a single sample, nothing is returned unless a start timestamp (ST) lies inside the window (`promql/functions.go:572-574`).
- **Start timestamp inside the window.** In that case the code assumes a zero-valued sample at the ST. It sets `durationToStart=0`, uses `sampledInterval = lastT - ST`, and adds `first.F` to the result (`promql/functions.go:551-571`).
- **Mixed samples.** A window with both float and histogram samples is dropped with a warning (`promql/functions.go:484-486`).
- **Native histograms.** They follow the same extrapolation, with `histogramRate` doing the reset-aware subtraction (`promql/functions.go:640` onward). The zero clamp uses the histogram `Count`.
- **Anchored or smoothed selectors.** These are routed to `extendedRate` or `extendedHistogramRate` instead (`promql/functions.go:455-466`).