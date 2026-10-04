**Short answer:** `rate()` and `increase()` both call `extrapolatedRate` in `promql/functions.go`. `funcRate` passes `isCounter=true, isRate=true` (`:811-813`). `funcIncrease` passes `isCounter=true, isRate=false` (`:816-818`). `increase` is the extrapolated counter increase over the window. `rate` is that same value divided by the range in seconds. I read the code but didn't run it, and I didn't open the helpers named below.

**Algorithm in `extrapolatedRate` (`promql/functions.go:452-~620`):**

1. **Window.** `rangeStart = enh.Ts - (Range + Offset)` and `rangeEnd = enh.Ts - Offset`.
2. **Special cases.**
   - Vector selectors using `anchored` or `smoothed` go to `extendedRate` or `extendedHistogramRate` instead (`:453-468`).
   - A window with both floats and histograms is dropped with a mixed-types warning.
3. **Raw delta.** For floats, `resultFloat = last.F - first.F`, with `firstT` and `lastT` taken from the first and last samples. Histograms use `histogramRate` instead.
4. **Counter resets (`isCounter` only).**
   - The loop walks consecutive pairs. If `curr.F < prev.F`, it adds `prev.F` to the result.
   - It also adds `prev.F` when `isStartTimestampReset` reports a reset from start timestamps. A start-time overlap produces a warning.
5. **Extrapolation to the window edges.**
   - `durationToStart = firstT - rangeStart` and `durationToEnd = rangeEnd - lastT`, both in seconds.
   - `sampledInterval = lastT - firstT`.
   - The average sample spacing is `sampledInterval / (n-1)`, and the threshold is 1.1 × that average.
   - If `durationToStart` or `durationToEnd` is at least the threshold, it is replaced by half the average spacing (`:~590-600`, `:~625`).
6. **Counters can't go negative.**
   - If `resultFloat > 0` and the first value is ≥ 0, then `durationToZero = sampledInterval * (first.F / resultFloat)`.
   - If that is less than `durationToStart`, the start extrapolation is capped at it (`:~602-618`).
   - Histograms do the same using `Count`.
7. **Start timestamps.**
   - If the first sample's start timestamp `sts[0]` lies inside the window (`sts[0] > rangeStart && sts[0] < firstT`), the code treats the counter as starting at zero then.
   - It sets `durationToStart = 0`, uses `sampledInterval = lastT - sts[0]`, and adds the first sample's value to the result.
   - This allows a result from a single sample.
   - Otherwise, a window with only one sample returns nothing.
8. **Scaling.**
   - `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, or 1 if `sampledInterval` is 0.
   - For `rate`, `factor /= ms.Range.Seconds()`.
   - The result is `resultFloat * factor`, or `resultHistogram.Mul(factor)` for histograms.

**Uncertainty:** The `~` line numbers are approximate because I read the code in a single printed block, not with line numbers. The function boundaries (`:452`, `:811`, `:816`) come from grep and are exact.