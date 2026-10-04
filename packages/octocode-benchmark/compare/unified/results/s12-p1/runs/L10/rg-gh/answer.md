`rate()` and `increase()` both call the same function, `extrapolatedRate` in `promql/functions.go:452`. They differ in one flag: `rate()` divides the result by the range in seconds, and `increase()` doesn't. I read the code but did not run it. I also did not confirm that the checkout is at ea954809ce, because `git rev-parse` failed with a `/dev/null` permission error.

**Entry points** (`promql/functions.go`)
- `funcRate` (line 811) calls `extrapolatedRate(..., isCounter=true, isRate=true)`.
- `funcIncrease` (line 816) calls `extrapolatedRate(..., isCounter=true, isRate=false)`.
- `funcDelta` (line 806) uses the same function with both flags false.

**Algorithm in `extrapolatedRate`**
1. **Window.** `rangeStart = enh.Ts - (range + offset)` and `rangeEnd = enh.Ts - offset`.
2. **Mixed samples.** If the window holds both float and histogram samples, the series is dropped and a mixed-floats-histograms warning is returned.
3. **Raw increase.** For floats, `resultFloat = last.F - first.F`.
4. **Counter resets.** When `isCounter` is true, the code walks consecutive pairs. If `curr.F < prev.F`, it adds `prev.F` to the result. It also treats a start-timestamp (ST) reset as a reset, via `isStartTimestampReset`. It adds a warning if the start timestamps overlap.
5. **Histograms.** These go through `histogramRate` instead of the float path.
6. **Extrapolation.**
   - `durationToStart = firstT - rangeStart` and `durationToEnd = rangeEnd - lastT`, both in seconds.
   - `sampledInterval = lastT - firstT`.
   - `averageDurationBetweenSamples = sampledInterval / (n-1)`.
   - `extrapolationThreshold = average * 1.1`.
   - If `durationToStart` or `durationToEnd` is at least the threshold, it is replaced by half the average spacing.
   - For counters, the extrapolation to the start is also capped at the point where the counter would reach zero: `durationToZero = sampledInterval * (first.F / resultFloat)`. This only applies when `resultFloat > 0` and `first.F >= 0`. The same idea is used for histograms with `Count`.
7. **Scaling.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`. If `isRate`, it is divided by `ms.Range.Seconds()`. The result is `resultFloat * factor`, or `resultHistogram.Mul(factor)` for histograms.

**Special cases**
- **Too few samples.** With fewer than two samples and no usable ST, no result is returned.
- **Start timestamp inside the window.** If the first sample's ST lies after `rangeStart` and before `firstT`, the code assumes a zero-valued sample at the ST. It sets `durationToStart = 0`, uses `lastT - ST` as the sampled interval, and adds the first sample's value to the result. This case can produce a result from a single sample.
- **Smoothed or anchored selectors.** These are handled by `extendedRate` or `extendedHistogramRate` instead. I did not read those two functions.

**Not verified:** the internals of `histogramRate`, `isStartTimestampReset`, `extendedRate` and `extendedHistogramRate`.