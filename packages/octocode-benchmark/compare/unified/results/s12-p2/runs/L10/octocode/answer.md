`rate()` and `increase()` are both thin wrappers around one function, `extrapolatedRate`, in `promql/functions.go`. `rate()` returns a per-second value and `increase()` returns the total over the window. I read the code only; I did not run it.

**Entry points**
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`functions.go:811-813`).
- `funcIncrease` calls it with `isCounter=true, isRate=false` (`functions.go:816-818`).
- If the selector uses the `anchored` or `smoothed` modifier, `extrapolatedRate` hands off to `extendedRate` or `extendedHistogramRate` instead (`functions.go:455-469`). I did not read those two functions.

**Float counters (`functions.go:508-534`)**
1. The raw increase is the last sample value minus the first. `firstT` and `lastT` are the timestamps of those two samples (`functions.go:509-512`).
2. Counter-reset correction runs over each consecutive pair of samples. If the current value is below the previous one, the previous value is added to the result. A start-timestamp reset also triggers this (`functions.go:521-534`).
3. The window is `rangeStart = enh.Ts - (ms.Range + offset)` to `rangeEnd = enh.Ts - offset` (`functions.go:474-475`).

**Extrapolation (`functions.go:540-632`)**
- `durationToStart` and `durationToEnd` are the gaps between the first and last samples and the window edges (`functions.go:541-542`).
- `sampledInterval` is `lastT - firstT`.
- The average sample spacing is `sampledInterval` divided by the number of samples minus one.
- The extrapolation threshold is 1.1 times that average spacing (`functions.go:544-549`).
- If a gap at either edge is at least the threshold, it is replaced by half the average spacing (`functions.go:588-590` for the start, `615-617` for the end).
- For counters, the start gap is also capped at the time at which the counter would have been zero. That time is `sampledInterval * (firstValue / resultFloat)`, and the cap only applies when the increase is positive and the first value is non-negative. This stops the extrapolation from implying negative counter values (`functions.go:591-612`).
- The scaling factor is `(sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or 1 if `sampledInterval` is 0 (`functions.go:619-622`).
- For `rate`, the factor is further divided by `ms.Range.Seconds()` (`functions.go:623-625`).
- The result is `resultFloat *= factor` (`functions.go:626-627`).

**Minimum samples**
- Normally at least two samples are needed. With only one sample and no usable start timestamp, nothing is returned (`functions.go:572-574`).
- With no samples at all, nothing is returned (`functions.go:535-537`).
- A window mixing float and histogram samples is dropped with a warning (`functions.go:484-486`).

**Start timestamps (ST)**
- If the first sample's start timestamp lies inside the window, the code assumes a zero-value sample at that timestamp. It adds the first sample's value to the result and sets `durationToStart = 0`. It also sets `sampledInterval = lastT - ST` (`functions.go:551-571`).
- Because of this branch, a single sample can produce a result when its start timestamp is inside the window (`functions.go:488-489`).

**Native histograms**
- The same flow applies, using `histogramRate` for the difference and reset handling (`functions.go:491-507`).
- The zero-point cap uses the histogram's `Count` (`functions.go:603-608`).
- The scaling is `resultHistogram.Mul(factor)` (`functions.go:629`).

**Not verified:** I did not read `histogramRate`, `extendedRate` or `extendedHistogramRate`, so I cannot say how they handle histogram resets or the anchored/smoothed cases.