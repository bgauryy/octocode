`rate()` and `increase()` are both thin wrappers around one function, `extrapolatedRate` in `promql/functions.go:452`. The only difference is the `isRate` flag. `funcRate` (`functions.go:811`) calls `extrapolatedRate(..., true, true)` and `funcIncrease` (`functions.go:816`) calls `extrapolatedRate(..., true, false)`. The arguments are `isCounter, isRate`. `delta()` uses `(false, false)`.

I read the float path in full. I did not read `histogramRate`, `extendedRate` or `extendedHistogramRate`, so histogram reset handling is only summarised below.

**How it computes the result** (line numbers are in `promql/functions.go`):

1. **Window bounds.** `rangeStart = enh.Ts - (Range + Offset)` and `rangeEnd = enh.Ts - Offset` (lines ~469–470).
2. **Raw delta.** For float samples, `resultFloat = last.F - first.F` (~502).
3. **Counter resets.** Only when `isCounter` is true, it loops over consecutive float pairs (~510–525). If `curr.F < prev.F`, it adds `prev.F` to the result. It also does this when a start-timestamp reset is detected via `isStartTimestampReset`, and it emits a start-time-overlap warning once per series.
4. **Extrapolation to the window edges** (~528–600):
   - It computes `durationToStart` (first sample to `rangeStart`), `durationToEnd` (last sample to `rangeEnd`), `sampledInterval` (last minus first timestamp), and the average spacing between samples.
   - The threshold is `averageDurationBetweenSamples * 1.1`.
   - If `durationToStart` or `durationToEnd` is at or above the threshold, that gap is replaced by half the average spacing. Otherwise the result is extrapolated all the way to the boundary.
   - For counters, the start extrapolation is also capped at the point where the counter would reach zero: `durationToZero = sampledInterval * (first.F / resultFloat)`. `durationToStart` is lowered to it if that is shorter. This applies when `resultFloat > 0` and the first value is non-negative.
5. **Scaling.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval` (or 1.0 if `sampledInterval` is 0). For `rate`, `factor /= ms.Range.Seconds()`. The result is `resultFloat *= factor`.
   - So `increase` is the extrapolated total increase over the window.
   - `rate` is that same value divided by the range in seconds, not by the sampled interval.

**Other cases in the same function:**
- **Sample count.** It normally needs at least two samples (`numSamplesMinusOne == 0` returns nothing). The exception is a single sample whose start timestamp (ST) falls inside the window. Then it assumes a zero-value sample at the ST, adds the first value to the result, and sets `durationToStart = 0`. This block is at ~534–548.
- **Mixed samples.** A window with both floats and histograms is dropped with a `NewMixedFloatsHistogramsWarning` (~478).
- **Histograms.** These go through `histogramRate` (defined ~644) and the same extrapolation, using `Count` for the zero-point cap.
- **Anchored or smoothed selectors.** These are routed to `extendedRate` or `extendedHistogramRate` instead (~455–468).