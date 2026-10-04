`rate()` and `increase()` are the same calculation. Both call `extrapolatedRate` in `promql/functions.go`, and only the final per-second division differs. I read the source at the checkout path. I did not run anything, and `git log` failed in this sandbox, so I couldn't confirm the checkout is at `ea954809ce`.

**Entry points** (`promql/functions.go:811-818`)
- `funcRate` calls `extrapolatedRate(..., isCounter=true, isRate=true)`.
- `funcIncrease` calls `extrapolatedRate(..., isCounter=true, isRate=false)`.
- `funcDelta` uses `isCounter=false, isRate=false`.

**Algorithm for the default case** (`extrapolatedRate`, starting at line 452)

1. **Raw delta.** `resultFloat = last.F - first.F` (line 507). This needs at least two samples. A single sample can still produce a result if a usable start timestamp (ST) lies inside the window (see step 4).
2. **Counter-reset correction** (lines 517-532).
   - It loops over consecutive pairs and adds `prevPoint.F` whenever `currPoint.F < prevPoint.F`. A drop is treated as a reset to zero.
   - It also counts as a reset when `isStartTimestampReset` says the start timestamp moved.
   - It adds an annotation if start-timestamp overlap is detected.
3. **Extrapolation to the window edges** (lines 535-600).
   - `durationToStart` is `firstT - rangeStart`, and `durationToEnd` is `rangeEnd - lastT`.
   - The average gap between samples is `sampledInterval / (n-1)`. The threshold is that average × 1.1.
   - If a gap to an edge is at least the threshold, it is replaced by half the average gap (lines 587-589 for the start, 598-600 for the end). Otherwise the result is extrapolated all the way to that edge.
   - For counters, the extrapolation to the start is also capped at the point where the counter would have been zero: `durationToZero = sampledInterval * (first.F / resultFloat)` (lines 591-609). This applies only if `resultFloat > 0` and the first value is `>= 0`. It prevents extrapolating to negative values.
4. **Start timestamp inside the window** (line 551). If the first sample's ST is after `rangeStart` and before `firstT`, the code assumes a zero-valued sample at the ST.
   - It sets `durationToStart = 0`, uses `lastT - ST` as the sampled interval, and adds the first value to the result.
   - A single sample with no usable ST returns nothing.
5. **Scaling.** `factor = (sampledInterval + durationToStart + durationToEnd) / sampledInterval`, which is 1 if `sampledInterval` is 0. For `rate`, `factor /= ms.Range.Seconds()`. The result is `resultFloat *= factor`.
   - So `increase` is the extrapolated delta over the window.
   - `rate` is that same value divided by the range in seconds.

**Native histograms.** The same flow applies. `histogramRate` (line 641) computes the delta and reset handling, and the result is scaled with `resultHistogram.Mul(factor)`. A window that mixes floats and histograms returns nothing and adds a warning (lines 487-490).

**Anchored and smoothed selectors.** These skip the extrapolation. `extrapolatedRate` hands them to `extendedRate` (lines 309-358) or `extendedHistogramRate` (line 374).
- They interpolate or pick the values at `rangeStart` and `rangeEnd` (`pickOrInterpolateLeft` and `pickOrInterpolateRight`, lines 72-90).
- The result is `right - left` plus `correctForCounterResets` over the samples strictly inside the window.
- `rate` divides by `ms.Range.Seconds()`.

**Not verified:** the helpers `isStartTimestampReset` and `correctForCounterResets` internals, and the `histogramRate` body. I only saw their call sites and signatures.