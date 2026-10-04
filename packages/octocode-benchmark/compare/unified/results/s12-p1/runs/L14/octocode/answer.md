`debounce` calls the wrapped function on a timer. By default it calls it once, on the trailing edge, after `wait` ms with no new calls. `maxWait` caps how long continuous calls can postpone that. All of this is in `lodash.js`, `debounce` at lines 10403–10526.

**Options (10418–10424)**
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing` is true whenever the `'maxWait'` key is present in `options`, even if its value is undefined.
- When `maxing` is true, `maxWait = max(toNumber(options.maxWait) || 0, wait)`, so `maxWait` is never below `wait`.

**Deciding to invoke: `shouldInvoke(time)` (10455–10464)**
It returns true if any of these hold:
- It is the first call (`lastCallTime === undefined`).
- `time - lastCallTime >= wait`, meaning activity has stopped.
- `time - lastCallTime < 0`, meaning the clock went backwards.
- `maxing && time - lastInvokeTime >= maxWait`.

**Each call to `debounced` (10499–10522)**
1. It evaluates `shouldInvoke(now())`, then stores `lastArgs`, `lastThis` and `lastCallTime`.
2. If `shouldInvoke` is true and no timer is running, it calls `leadingEdge`. That sets `lastInvokeTime` and starts a `wait` timer. It calls `func` immediately only if `leading` is true.
3. If `shouldInvoke` is true, a timer is already running and `maxing` is set, it restarts the timer and calls `func` immediately via `invokeFunc` (10511–10515). The code comments this as handling "invocations in a tight loop".
4. Otherwise it starts a `wait` timer if none is running and returns the last `result`.

**Timer: `timerExpired` (10466–10473)**
- When the timer fires, it calls `trailingEdge` if `shouldInvoke` is true.
- If not, it reschedules for `remainingWait(time)`.
- `remainingWait` (10445–10453) is `wait - timeSinceLastCall`. With `maxing`, it is the smaller of that and `maxWait - timeSinceLastInvoke`.
- This is how `maxWait` forces a call even while calls keep arriving.

**Trailing edge (10475–10485)**
- `trailingEdge` calls `func` only if `trailing` is true and `lastArgs` is set. `lastArgs` is cleared by every invoke, so a leading call with no further calls does not trigger a second one.
- Otherwise it clears `lastArgs` and `lastThis` and returns the last `result`.

**`maxWait` in practice**
- Without `maxWait`, a steady stream of calls spaced less than `wait` apart keeps pushing the trailing call back indefinitely.
- With `maxWait`, `func` is invoked at least once per `maxWait` ms of continuous calling, measured from `lastInvokeTime`.

**Other methods**
- `cancel` (10487–10493) clears the timer and resets state.
- `flush` (10495–10497) runs `trailingEdge` immediately if a timer is pending.

I read the source only. I did not run it or any tests.