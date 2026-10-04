`debounce` calls `func` from timers, using a `shouldInvoke` check. It runs on the trailing edge (after `wait` ms of quiet), optionally on the leading edge, and with `maxWait` it is also forced to run at least once per `maxWait` ms during continuous calls. All lines below are in `lodash.js` at the pinned commit.

**Setup (10418–10424)**
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false, and `trailing` defaults to true. They are overridden only through `options`.
- `maxing = 'maxWait' in options`, and `maxWait = max(toNumber(options.maxWait) || 0, wait)`. So `maxWait` can never be smaller than `wait`.

**Each call to `debounced()` (10499–10522)**
- It records `lastArgs`, `lastThis` and `lastCallTime`, then evaluates `shouldInvoke(now)`.
- If `shouldInvoke` is true and no timer is running, it calls `leadingEdge`. That sets `lastInvokeTime`, starts a `wait` timer, and invokes `func` immediately only if `leading` is true (10436–10443).
- If `shouldInvoke` is true, a timer is already running, and `maxing` is set, it restarts the timer and invokes `func` right away (10511–10516). The comment says this handles invocations in a tight loop.
- Otherwise, if no timer exists, it starts a `wait` timer. It returns the last `result`.

**`shouldInvoke` (10455–10464)** is true when any of these holds:
- It is the first call (`lastCallTime === undefined`).
- `timeSinceLastCall >= wait`, meaning activity has stopped.
- `timeSinceLastCall < 0`, meaning the clock went backwards.
- `maxing && timeSinceLastInvoke >= maxWait`.

**Timer expiry (10466–10473)**
- `timerExpired` calls `trailingEdge` if `shouldInvoke` is true.
- Otherwise it reschedules itself for `remainingWait(time)`. That is `wait - timeSinceLastCall`, or, when `maxing`, the smaller of that and `maxWait - timeSinceLastInvoke` (10445–10453).

**Trailing edge (10475–10485)**
- `trailingEdge` invokes `func` only if `trailing && lastArgs`. `lastArgs` is cleared after every invocation, so a call that already ran on the leading edge doesn't run again unless the debounced function was called again.

**What `maxWait` does**
- Without `maxWait`, the timer keeps being pushed back while calls keep arriving, so `func` can be starved indefinitely.
- With `maxWait`, `remainingWait` shortens the timer and `shouldInvoke` forces an invocation once `maxWait` ms have passed since the last invoke. The invocation uses the latest args.

**Other behaviour**
- `cancel` clears the timer and all state (10487–10493).
- `flush` runs `trailingEdge` immediately if a timer is pending (10495–10497).

I read only the source; I did not run it or check the tests.