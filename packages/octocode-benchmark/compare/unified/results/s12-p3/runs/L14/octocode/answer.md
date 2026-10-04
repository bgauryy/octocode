`debounce` calls `func` only from a timer callback or from the very first call of a burst. A call by itself never runs `func` unless it qualifies as the leading edge, or `maxWait` is set and has been exceeded. `maxWait` caps how long a steady stream of calls can postpone the invocation. All lines below are in `lodash.js` at the pinned commit.

**Options** (`lodash.js:10418-10424`)
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing` is true whenever the `'maxWait'` key is present in `options`.
- `maxWait` is set to `max(toNumber(options.maxWait) || 0, wait)`, so it can never be less than `wait`.

**The decision on each call** (`debounced`, `lodash.js:10499-10522`)
1. The call records `lastArgs`, `lastThis` and `lastCallTime`. `shouldInvoke(time)` is evaluated before these are updated.
2. `shouldInvoke` (`lodash.js:10455-10464`) is true if any of these holds:
   - It is the first call (`lastCallTime === undefined`).
   - `wait` or more has passed since the last call.
   - The clock went backwards (`timeSinceLastCall < 0`).
   - `maxing` is on and `maxWait` or more has passed since the last invoke.
3. If `shouldInvoke` is true and no timer is running, `leadingEdge` runs (`lodash.js:10436-10443`). It sets `lastInvokeTime`, starts a `wait` timer, and calls `func` immediately only if `leading` is true. Otherwise it returns the previous `result`.
4. If `shouldInvoke` is true, a timer is already running and `maxing` is on, the timer is restarted and `func` is invoked immediately (`lodash.js:10511-10516`). This is the `maxWait` case: calls keep arriving without a pause, so the `wait` timer keeps being pushed back.
5. Otherwise, if no timer is running, one is started for `wait`. The call returns the last `result`.

**Timer path** (`lodash.js:10466-10485`)
- `timerExpired` checks `shouldInvoke(now())`. If it is true, `trailingEdge` runs.
- If it is false, the timer is rescheduled for `remainingWait`. That is `wait - timeSinceLastCall`, or, with `maxing`, the smaller of that and `maxWait - timeSinceLastInvoke` (`lodash.js:10445-10453`).
- `trailingEdge` clears `timerId`. It invokes `func` only if `trailing` is true and `lastArgs` is set. `lastArgs` is cleared after every invoke, so a leading-only call doesn't fire a second time.

**Other methods**
- `cancel` clears the timer and resets all state (`lodash.js:10487-10493`).
- `flush` runs `trailingEdge` immediately if a timer is pending, and otherwise returns `result` (`lodash.js:10495-10497`).

Everything above comes from reading the code. I didn't run it.