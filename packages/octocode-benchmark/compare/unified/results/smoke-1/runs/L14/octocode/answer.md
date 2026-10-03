I read the source in `lodash.js` in the local checkout, `debounce` at lines 10403–10527. I did not run it. The line numbers below come from the read range starting at 10403 (`debounce` itself, line 10403) and are approximate offsets within it.

**Short answer:** `debounce` never calls `func` directly. Each call to the returned `debounced()` only records the latest args, `this` and call time. A `setTimeout` timer then decides when `func` runs, using `shouldInvoke(time)`. `maxWait` puts an upper limit on how long calls can keep postponing `func`.

**Setup (10403–10421)**
- `wait` is coerced with `toNumber(wait) || 0`.
- Options: `leading` defaults to false and `trailing` defaults to true.
- `maxing = 'maxWait' in options`. When it's set, `maxWait = nativeMax(toNumber(options.maxWait) || 0, wait)`, so `maxWait` is never smaller than `wait`.

**When `func` runs**
1. **`debounced()`** (10499): computes `isInvoking = shouldInvoke(time)`, then stores `lastArgs`, `lastThis` and `lastCallTime`.
   - If `isInvoking` is true and there is no timer, it calls `leadingEdge`.
   - `leadingEdge` sets `lastInvokeTime`, starts `setTimeout(timerExpired, wait)`, and calls `func` immediately only if `leading` is true.
   - If a timer is already running and `maxing` is set, it restarts the timer and calls `invokeFunc` immediately. The source comment says this handles invocations in a tight loop.
   - Otherwise, if no timer is running, it starts one for `wait`. It returns the last `result`.
2. **`shouldInvoke(time)`** is true if any of these hold:
   - It is the first call (`lastCallTime === undefined`).
   - `timeSinceLastCall >= wait`.
   - The clock went backwards (`timeSinceLastCall < 0`).
   - `maxing && timeSinceLastInvoke >= maxWait`.
3. **`timerExpired`** reads the current time. If `shouldInvoke` is true, it runs `trailingEdge`. Otherwise it re-arms the timer with `remainingWait(time)`.
4. **`remainingWait`** is `wait - timeSinceLastCall`. With `maxing` it is the smaller of that and `maxWait - timeSinceLastInvoke`. This makes the timer fire no later than `maxWait` after the last invocation.
5. **`trailingEdge`** clears `timerId`. It calls `invokeFunc` only if `trailing && lastArgs`, meaning at least one call has happened since the last invocation. Otherwise it clears the stored args.
6. **`invokeFunc`** runs `func.apply(lastThis, lastArgs)`, clears the args and records `lastInvokeTime`.

**What `maxWait` does:** without it, `func` fires only after `wait` ms of quiet, so a steady stream of calls can delay it forever. With it, `func` is forced to run once `maxWait` ms have passed since the last invocation, even if calls keep arriving. This works through the `shouldInvoke` clause and the shortened `remainingWait`.

**Other methods:**
- `cancel` clears the timer and resets state, including `lastInvokeTime = 0`.
- `flush` calls `trailingEdge(now())` if a timer is pending. Otherwise it returns the last `result`.

**Uncertainty:** none on the logic. Exact line numbers for inner functions were not shown individually, only the range.