`rate()` and `increase()` share one implementation, `extrapolatedRate`, in `promql/functions.go`. `funcRate` calls it with `isCounter=true, isRate=true` (`promql/functions.go:811-813`). `funcIncrease` calls it with `isCounter=true, isRate=false` (`promql/functions.go:816-818`). So `increase` is the extrapolated counter increase over the window, and `rate` is the same value divided by the range in seconds. I read the code but did not run it.

**Steps for float samples (default path, `promql/functions.go:452-635`)**
1. **Window:** `rangeStart = enh.Ts - (range + offset)` and `rangeEnd = enh.Ts - offset` (`:471-472`).
2. **Sample count:** at least two samples are needed. The exception is a single sample whose start timestamp (ST) lies inside the window (see the ST bullet below). With one sample and no usable ST, nothing is returned (`:606-608`).
3. **Raw delta:** `last.F - first.F` (`:509`).
4. **Counter resets:** the loop at `:519-532` walks consecutive pairs. Whenever `curr.F < prev.F`, it adds `prev.F` to the result. This treats the counter as having restarted from 0. A reset is also counted when `isStartTimestampReset` reports one from the start timestamps.
5. **Extrapolation to the window edges (`:537-635`):**
   - `durationToStart = firstT - rangeStart` and `durationToEnd = rangeEnd - lastT`, both in seconds.
   - `sampledInterval = lastT - firstT`.
   - `averageDurationBetweenSamples = sampledInterval / (n-1)`.
   - `extrapolationThreshold = 1.1 * average` (`:546-548`).
   - If a gap to the start or end is at least the threshold, it is replaced by `average/2` (`:588-590`, `:614-616`). This assumes the series really starts or ends inside the window.
   - Counters cannot go negative, so the start extrapolation is also capped at the point where the counter would reach zero: `durationToZero = sampledInterval * (firstValue / resultFloat)`. If that is shorter than `durationToStart`, it becomes the new `durationToStart` (`:591-608`, applied for `resultFloat > 0` and `first.F >= 0`).
6. **Scaling:** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`. For `rate`, `factor /= ms.Range.Seconds()`. The result is `resultFloat * factor` (`:618-631`).

**Other cases**
- **Native histograms:** the same flow is used. The result comes from `histogramRate` (`:640` onward) and is scaled with `resultHistogram.Mul(factor)`. The zero-point cap uses the histogram `Count`. Mixed float and histogram samples in one window produce a warning and no result (`:485-487`).
- **Start timestamps (ST):** if the first sample's ST is inside the window (`sts[0] > rangeStart && sts[0] < firstT`), the code assumes a zero-value sample at the ST. It sets `durationToStart = 0`, adds the first value to the result, and uses `lastT - ST` as the sampled interval (`:551-566`). This lets a single sample produce a result, though right-side extrapolation is then lost.
- **Anchored and smoothed selectors:** these bypass the path above and use `extendedRate` or `extendedHistogramRate`. They interpolate values at the range boundaries instead of extrapolating (`:454-468`, `:309-358`).

**Uncertainty:** I did not read `histogramRate`, `extendedRate` or the start-timestamp helpers in detail, so my descriptions of them are based on the call sites and comments.