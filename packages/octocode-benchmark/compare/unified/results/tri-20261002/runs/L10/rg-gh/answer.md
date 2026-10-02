`rate()` and `increase()` both call the same function, `extrapolatedRate`, in `promql/functions.go`. I read the source only and did not run any queries. The shell could not run `git`, so I did not confirm that the checkout is at `ea954809ce`.

**Entry points**
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)` (`promql/functions.go:811-813`).
- `funcIncrease` calls `extrapolatedRate(..., isCounter=true, isRate=false)` (`promql/functions.go:816-818`).
- `delta` uses the same function with `isCounter=false` (`promql/functions.go:805-808`).

**Algorithm in `extrapolatedRate` (`promql/functions.go:452-636`)**
1. **Window bounds.** `rangeStart = enh.Ts - (range + offset)` and `rangeEnd = enh.Ts - offset` (`:469-470`).
2. **Mixed samples.** If the window holds both floats and native histograms, it returns no value and adds a mixed-floats-histograms warning (`:478-480`).
3. **Smoothed or anchored selectors.** These are handled by `extendedRate` or `extendedHistogramRate` instead (`:454-468`).
4. **Raw delta.** For floats, `resultFloat = last.F - first.F` (`:511`).
5. **Counter resets.** When `isCounter` is true, it walks consecutive pairs. If `curr.F < prev.F`, it adds `prev.F` to the result (`:524-535`).
   - A reset is also detected when start timestamps (STs) indicate one, via `isStartTimestampReset`.
   - It also warns when STs overlap (`:526-531`).
6. **Histograms.** These use `histogramRate` (`:640`) to compute the delta, with the same reset handling.
7. **Extrapolation.**
   - It computes `durationToStart` and `durationToEnd` from the first and last sample to the window edges (`:546-547`).
   - `sampledInterval = lastT - firstT`.
   - `averageDurationBetweenSamples = sampledInterval / (n-1)`.
   - `extrapolationThreshold = average * 1.1` (`:549-554`).
   - If `durationToStart` or `durationToEnd` is at least the threshold, that side is extrapolated by only half the average spacing (`:597-599`, `:620-622`).
   - Otherwise it extrapolates all the way to the window edge.
8. **Counter zero clamp.** If the counter is rising and the first value is non-negative, it computes `durationToZero = sampledInterval * (first.F / resultFloat)`. If that is shorter than `durationToStart`, it uses it, so the extrapolation never goes below zero (`:601-618`). Histograms use `Count` for the same clamp.
9. **Final scaling.**
   - `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or 1 if `sampledInterval` is 0 (`:624-627`).
   - For `rate`, `factor /= range.Seconds()` (`:628-630`).
   - The result is `resultFloat *= factor`, or `resultHistogram.Mul(factor)` for histograms (`:631-635`).

So `increase` is the reset-corrected delta scaled up to cover the whole window, and `rate` is that value divided by the range in seconds.

**Start-timestamp special case (`:556-596`)**
- If the first sample has a start timestamp `sts[0]` with `rangeStart < sts[0] < firstT`, the code treats the counter as starting from zero at that time.
- In that case it sets `durationToStart = 0`, uses `lastT - sts[0]` as `sampledInterval`, and adds the first sample's value to the result.
- Because of this, a single sample is enough when a usable start timestamp exists.
- Without a usable start timestamp, fewer than two samples produce no result (`:587-590`).

**Not verified**
- I did not read `extendedRate`, `extendedHistogramRate`, `histogramRate`, or `isStartTimestampReset` in detail, so I haven't described their internals.