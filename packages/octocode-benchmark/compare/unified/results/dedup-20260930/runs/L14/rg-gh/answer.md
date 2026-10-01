`debounce` calls `func` from two places. It calls on the leading edge if `leading` is true, and on the trailing edge once calls have been quiet for `wait` ms. `maxWait` caps how long a steady stream of calls can postpone `func`. All line numbers are in `lodash.js` at 2b5e6f7399.

**Setup (`lodash.js:10418-10424`)**
- `wait` is coerced with `toNumber(wait) || 0`.
- The defaults are `leading=false` and `trailing=true`.
- `maxing = 'maxWait' in options`. If it is set, `maxWait = max(toNumber(options.maxWait) || 0, wait)`, so `maxWait` can never be smaller than `wait`.

**What happens on each call (`debounced`, 10499-10522)**
1. It computes `isInvoking = shouldInvoke(now)`. It then stores `lastArgs`, `lastThis` and `lastCallTime`, so the most recent call's arguments are the ones used.
2. If `isInvoking` is true and no timer is running (`timerId === undefined`), it runs `leadingEdge` (10436-10443). That sets `lastInvokeTime` and starts a `wait` timer. It only calls `func` right away if `leading` is true. Otherwise it returns the previous `result`.
3. If `isInvoking` is true, a timer is running and `maxing` is on, it restarts the timer and calls `func` immediately (10511-10515). The comment says this handles invocations in a tight loop.
4. Otherwise it starts a `wait` timer if none is running and returns the previous `result`.

**`shouldInvoke` (10455-10464)** returns true when any of these holds:
- It is the first call (`lastCallTime === undefined`).
- `timeSinceLastCall >= wait`.
- The clock went backwards (`timeSinceLastCall < 0`).
- `maxing && timeSinceLastInvoke >= maxWait`.

**Timer and trailing edge**
- `timerExpired` (10466-10473) re-checks `shouldInvoke`. If it is true, it runs `trailingEdge`. If not, it reschedules the timer for `remainingWait`.
- `remainingWait` (10445-10453) is `wait - timeSinceLastCall`. With `maxing` it is `min(that, maxWait - timeSinceLastInvoke)`, so the timer wakes up no later than the `maxWait` deadline.
- `trailingEdge` (10475-10485) calls `func` only if `trailing` is true and `lastArgs` is set. `lastArgs` is cleared after each invocation, so a leading call that used the same arguments is not repeated on the trailing edge.

**What `maxWait` does**
- Without `maxWait`, `func` can be delayed forever if calls keep arriving less than `wait` ms apart.
- With `maxWait`, `func` is invoked at least once per `maxWait` ms while calls continue. The `timeSinceLastInvoke >= maxWait` check in `shouldInvoke` triggers that invocation.
- `lastInvokeTime` is measured from the last actual invocation. `leadingEdge` also resets it.

**Other controls**
- `cancel` (10487-10493) clears the timer and all state.
- `flush` (10495-10497) runs `trailingEdge` immediately if a timer is pending. Otherwise it returns the last `result`.

I read the source only and did not run any of it.