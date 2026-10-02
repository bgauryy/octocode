`rate()` and `increase()` share one function, `extrapolatedRate`, in `promql/functions.go:452`. I read that function and the helpers near it. I did not read `extendedRate` or `histogramRate`, so their internals are not covered here.

**Entry points**
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`functions.go:811-813`).
- `funcIncrease` calls it with `isCounter=true, isRate=false` (`functions.go:816-818`).
- The only difference is the final division by the range length.

**Float counters, default path**
1. **Window bounds.** `rangeStart = enh.Ts - (Range + Offset)` and `rangeEnd = enh.Ts - Offset` (`:474-475`).
2. **Raw delta.** `resultFloat = last.F - first.F` (`:512`).
3. **Counter resets.** The code loops over consecutive pairs. When `curr.F < prev.F`, it adds `prev.F` to the result (`:521-534`). That treats the counter as having restarted from 0.
4. **Start-timestamp resets.** A reset is also counted when `isStartTimestampReset(...)` fires. `checkStartTimeOverlap` produces a warning annotation instead (`:524-531`).
5. **Too few samples.** With a single sample and no useful start timestamp, nothing is returned (`:572-574`). With no float or histogram samples, nothing is returned either (`:535-537`).

**Extrapolation**
- `durationToStart = (firstT - rangeStart)/1000` and `durationToEnd = (rangeEnd - lastT)/1000` (`:541-542`).
- `sampledInterval = (lastT - firstT)/1000`.
- `averageDurationBetweenSamples = sampledInterval / (n-1)`, and `extrapolationThreshold = average * 1.1` (`:544-549`).
- If a gap to the start or end is at least the threshold, it is replaced by `average/2` (`:588-590`, `:615-617`). Otherwise the series is extrapolated all the way to the range boundary.
- For counters, the start extrapolation is also capped so the value does not go below zero. `durationToZero = sampledInterval * (firstValue / resultFloat)`, and the start gap is `min(durationToStart, durationToZero)` (`:598-611`). This applies only when `resultFloat > 0` and the first value is `>= 0`.
- `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or `1` if `sampledInterval == 0` (`:619-622`).
- For `rate`, `factor /= ms.Range.Seconds()` (`:623-625`).
- The result is `resultFloat * factor` (`:626-632`).

**Start-timestamp special case (`:551-571`)**
- If the first sample's start timestamp lies strictly inside the window (`rangeStart < st < firstT`), the code assumes a zero-valued sample at that timestamp.
- It sets `durationToStart = 0`, uses `lastT - st` as the sampled interval, and adds the first sample's value to the result.

**Native histograms**
- With two or more histogram samples, `histogramRate` computes the delta (`:499-507`). Per `:438-442`, the same extrapolation factor is applied and the result's `CounterResetHint` is set to `GaugeType`.
- A window mixing float and histogram samples drops the element and adds a warning (`:484-486`).

**Smoothed and anchored selectors**
- If the selector uses `smoothed` or `anchored`, the code delegates to `extendedRate` or `extendedHistogramRate` instead (`:455-469`).
- Those use helpers that interpolate the boundary values and compute reset correction with `correctForCounterResets` (`:69-178`).

**Uncertainty:** I did not trace how `enh.StartTimestamps` is populated, or the details of `isStartTimestampReset`.