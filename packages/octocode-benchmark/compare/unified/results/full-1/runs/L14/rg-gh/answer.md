`debounce` calls `func` on the leading edge, the trailing edge, or when `maxWait` forces a call. All of this is in `lodash.js:10403-10526`, and `HEAD` matches the pinned commit. `maxWait` puts a ceiling on how long continuous calls can postpone an invocation.

**Options** (`lodash.js:10418-10424`)
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing` is true whenever the key `'maxWait'` is present in `options`. The check is `'maxWait' in options` (line 10421).
- `maxWait` is `max(toNumber(options.maxWait) || 0, wait)` (line 10422), so it can never be smaller than `wait`.

**Deciding whether to invoke: `shouldInvoke(time)`** (lines 10455-10464)
It returns true if any of these hold:
- It is the first call (`lastCallTime === undefined`).
- `timeSinceLastCall >= wait`, meaning calls have been quiet for `wait` ms. This is the trailing edge.
- `timeSinceLastCall < 0`, meaning the clock went backwards.
- `maxing && timeSinceLastInvoke >= maxWait`.

**On each call: `debounced()`** (lines 10499-10522)
1. It computes `isInvoking = shouldInvoke(now)`, then stores `lastArgs`, `lastThis` and `lastCallTime`.
2. If `isInvoking` is true and there is no timer, it calls `leadingEdge` (lines 10507-10509). `leadingEdge` sets `lastInvokeTime`, starts a `setTimeout(timerExpired, wait)`, and calls `func` immediately only if `leading` is true (lines 10436-10443).
3. If `isInvoking` is true, a timer is already running, and `maxing` is set, it restarts the timer and calls `func` right away (lines 10511-10515). The comment says this handles "invocations in a tight loop".
4. Otherwise it starts a timer if none is running and returns the last `result` (lines 10518-10521). `func` is not called on this path.

**Timer: `timerExpired`** (lines 10466-10473)
- When the timer fires, it calls `trailingEdge` if `shouldInvoke` is true.
- If not, it reschedules itself with `remainingWait` (line 10472).
- `remainingWait` is `min(wait - timeSinceLastCall, maxWait - timeSinceLastInvoke)` when `maxing`, and `wait - timeSinceLastCall` otherwise (lines 10445-10453). With `maxWait`, the timer therefore wakes up in time to hit the `maxWait` deadline.

**Trailing edge: `trailingEdge`** (lines 10475-10485)
- It clears `timerId`.
- It calls `func` only if `trailing && lastArgs`. `lastArgs` is set only if `debounced` was called since the last invocation.
- Otherwise it drops the stored args and returns the previous `result`.

**Other methods**
- `cancel` clears the timer and resets all state (lines 10487-10493).
- `flush` runs `trailingEdge` immediately if a timer is pending. Otherwise it returns `result` (lines 10495-10497).

**Summary of `maxWait`:** without it, a steady stream of calls spaced less than `wait` apart never triggers `func`, because each call pushes the trailing edge back. With it, `func` is invoked at least once every `maxWait` ms (measured from `lastInvokeTime`) while calls continue.

I read the code and did not run it, so the behaviour above is from the source alone.