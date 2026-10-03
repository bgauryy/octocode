`rate()` and `increase()` both go through one function, `extrapolatedRate` in `promql/functions.go:452`. `funcRate` (`functions.go:811`) calls it with `isCounter=true, isRate=true`. `funcIncrease` (`functions.go:816`) calls it with `isCounter=true, isRate=false`. The only difference is a final division by the range in seconds for `rate`. I read the source only; I did not run any queries or tests.

**Float samples (`functions.go:452-680`)**
1. The raw increase is `last.F - first.F` (~line 508).
2. Counter resets are corrected in a loop over consecutive pairs. If `curr.F < prev.F`, or a start-timestamp reset is detected, it adds `prev.F` to the result.
3. It computes `durationToStart` and `durationToEnd`, the gaps between the first and last sample and the range boundaries. `rangeStart` and `rangeEnd` are `enh.Ts` minus the range and offset.
4. It computes `sampledInterval = lastT - firstT` and `averageDurationBetweenSamples = sampledInterval / (n-1)`. The extrapolation threshold is 1.1 times that average.
5. If a gap is at least the threshold, it is replaced by half the average sample spacing. Otherwise the result is extrapolated all the way to the range boundary.
6. For counters, the start extrapolation is capped at the point where the counter would reach zero: `durationToZero = sampledInterval * (first.F / resultFloat)`. This applies only when `resultFloat > 0` and `first.F >= 0`. It avoids extrapolating to negative values.
7. `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or 1 if `sampledInterval` is 0. For `rate`, `factor` is then divided by `ms.Range.Seconds()`. The result is `resultFloat * factor`.

**Fewer than two samples**
- With a single sample and no usable start timestamp, nothing is returned.
- If the first sample's start timestamp (ST) lies inside the range, the code treats it as a zero-valued sample at ST. It sets `durationToStart = 0`, uses `lastT - ST` as the interval, and adds the first sample's value to the result.

**Native histograms**
- `histogramRate` computes `last - first` with reset handling (`DetectReset`, start-timestamp reset). It works on the smallest schema across the samples.
- The same extrapolation factor is then applied with `Mul`.
- Mixed float and histogram samples in one window drop the series and emit a warning.

**Smoothed or anchored selectors**
- These are delegated to `extendedRate` or `extendedHistogramRate` (~lines 456-470). I did not read those functions.

**Not verified**
- The exact line of the `resultFloat` subtraction is approximate.
- The ~line 435-451 helper that ends with `resultHistogram.Div(ms.Range.Seconds())` is a separate function that I did not identify.