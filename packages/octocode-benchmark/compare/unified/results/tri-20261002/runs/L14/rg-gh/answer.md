**Short answer:** `debounce` in `lodash.js` calls `func` on the trailing edge, after `wait` ms with no new calls. It also calls on the leading edge if `leading: true`. With `maxWait`, it forces a call once `maxWait` ms have passed since the last invocation, even if calls keep arriving. I read the code but did not run it, and I did not confirm the checkout is at the pinned commit. `git rev-parse` failed in this sandbox with a `/dev/null` permission error.

**Options** (`lodash.js:10403-10421`)
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing` is true if the `'maxWait'` key is present in `options`.
- `maxWait` is `max(toNumber(options.maxWait) || 0, wait)`, so it can never be smaller than `wait`.

**The decision function, `shouldInvoke(time)`** (`lodash.js:10450-10459`)

It returns true when any of these holds:
- It is the first call (`lastCallTime === undefined`).
- `timeSinceLastCall >= wait`, meaning activity has stopped.
- `timeSinceLastCall < 0`, meaning the system clock went backwards.
- `maxing && timeSinceLastInvoke >= maxWait`.

**What happens on each call** (`debounced`, `lodash.js:10499-10521`)
1. It computes `isInvoking = shouldInvoke(now())`, then stores `lastArgs`, `lastThis` and `lastCallTime`.
2. If `isInvoking` is true and no timer is running, it calls `leadingEdge`. That sets `lastInvokeTime` and starts a `wait` ms timer. It calls `func` right away only if `leading` is true.
3. If `isInvoking` is true, a timer is already running and `maxing` is set, it restarts the timer and calls `func` immediately. The code comment calls this handling "invocations in a tight loop".
4. Otherwise it starts a timer if none exists and returns the last `result`.

**Timer expiry** (`timerExpired`, `lodash.js:10461-10468`)
- If `shouldInvoke` is true at expiry, it calls `trailingEdge`.
- Otherwise it re-arms the timer for `remainingWait(time)`.
- `remainingWait` (`lodash.js:10442-10448`) is `min(wait - timeSinceLastCall, maxWait - timeSinceLastInvoke)` when `maxing`, and just `wait - timeSinceLastCall` otherwise. This is how `maxWait` makes the timer fire early.

**Trailing edge** (`trailingEdge`, `lodash.js:10470-10480`)
- `func` is invoked only if `trailing` is true and `lastArgs` is set, so there has been a call since the last invocation.
- `invokeFunc` (`lodash.js:10423-10431`) calls `func.apply(lastThis, lastArgs)`, clears the saved args and records `lastInvokeTime`.
- With `leading: true` and a single call, the saved args are already cleared by the leading invoke, so there is no second call.

**What `maxWait` does**
- Without `maxWait`, constant calls spaced closer than `wait` postpone `func` indefinitely.
- With `maxWait`, `func` is guaranteed to run at least once every `maxWait` ms while calls continue, because `shouldInvoke` turns true once `timeSinceLastInvoke >= maxWait`.

**Helpers** (`lodash.js:10482-10497`)
- `cancel` clears the timer and resets all state.
- `flush` calls `trailingEdge` immediately if a timer is pending, and otherwise returns the last `result`.