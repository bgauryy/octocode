`debounce` calls `func` on the leading edge (if `leading` is set), on the trailing edge after `wait` ms of quiet (if `trailing` is set, the default), or when `maxWait` forces a call. All line numbers below are in `lodash.js` at the pinned commit.

**Setup (10418–10424)**
- `wait` is coerced to a number, defaulting to 0.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing = 'maxWait' in options` — merely having the key enables max-wait mode.
- `maxWait = max(toNumber(options.maxWait) || 0, wait)`, so it can never be smaller than `wait`.

**Each call to `debounced` (10499–10522)**
1. It computes `isInvoking = shouldInvoke(now)`, then records `lastArgs`, `lastThis` and `lastCallTime`.
2. If `isInvoking` is true and no timer is running, it calls `leadingEdge` (10436). That sets `lastInvokeTime` and starts a `wait` timer. It calls `func` right away only if `leading` is true, and otherwise returns the previous `result`.
3. If `isInvoking` is true, a timer is already running and `maxing` is on, it restarts the timer and calls `func` immediately (10511–10516). The code comments this as handling "invocations in a tight loop".
4. Otherwise it starts a timer if none exists and returns the previous `result`.

**`shouldInvoke` (10455–10464)**
It returns true when any of these holds:
- This is the first call (`lastCallTime === undefined`).
- `timeSinceLastCall >= wait`.
- The clock went backwards (`timeSinceLastCall < 0`).
- `maxing && timeSinceLastInvoke >= maxWait`.

**Timer (10466–10485)**
- `timerExpired` re-checks `shouldInvoke(now())`. If it is true, it runs `trailingEdge`.
- If it is false, it re-arms the timer for `remainingWait(time)` (10445–10453). That is `wait - timeSinceLastCall`, or with `maxing`, the smaller of that and `maxWait - timeSinceLastInvoke`.
- `trailingEdge` clears `timerId` and invokes `func` only if `trailing && lastArgs`. `lastArgs` is only set when `debounced` has been called since the last invocation, so a leading-only call is not repeated.

**What `maxWait` does**
Without `maxWait`, constant calls keep pushing the trailing call back indefinitely. With it, `func` is guaranteed to run at least once every `maxWait` ms. `remainingWait` shortens the timer so it fires by that deadline, and `shouldInvoke` forces the invocation once `timeSinceLastInvoke >= maxWait`.

**Other methods**
`cancel` (10487) clears the timer and all state. `flush` (10495) runs `trailingEdge` immediately if a timer is pending, and otherwise returns the last `result`.

Everything above is from reading the source. I did not run any of it.