I read `lodash.js` in the checkout, but I couldn't confirm the checkout is at 2b5e6f7399. `git rev-parse` failed because `/dev/null` is blocked in this sandbox. I also didn't run the code.

**Short answer:** `debounce` (`lodash.js:10403`) calls `func` on the trailing edge. That means it calls `func` once calls have stopped for `wait` ms. With `leading: true` it also calls on the first call of a burst. `maxWait` sets an upper limit on how long `func` can be postponed while calls keep arriving.

**Options** (`lodash.js:10404-10424`)
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true.
- `maxing = 'maxWait' in options`. The check is on key presence, not on the value.
- `maxWait = max(toNumber(options.maxWait) || 0, wait)`, so it is never smaller than `wait`.

**Decision logic**
- **`shouldInvoke(time)`** (10455-10464) returns true if any of these hold:
  - It is the first call (`lastCallTime === undefined`).
  - `timeSinceLastCall >= wait`, meaning calls have been quiet for `wait`.
  - `timeSinceLastCall < 0`, meaning the clock went backwards.
  - `maxing && timeSinceLastInvoke >= maxWait`.
- **`debounced()`** (10499-10522):
  1. It records `lastArgs`, `lastThis` and `lastCallTime`.
  2. If `shouldInvoke` is true and no timer is running, it calls `leadingEdge`. That sets `lastInvokeTime`, starts a `wait` timer, and calls `func` immediately only if `leading` is true (10436-10443).
  3. If `shouldInvoke` is true, a timer is already running and `maxing` is set, it restarts the timer and calls `func` immediately (10511-10516). The source comment says this handles invocations in a tight loop.
  4. Otherwise it starts a timer if none exists and returns the previous `result`.
- **`timerExpired`** (10466-10473) re-checks `shouldInvoke`.
  - If true, it runs `trailingEdge`.
  - If false, it re-arms the timer for `remainingWait(time)`.
- **`remainingWait`** (10445-10453) returns `wait - timeSinceLastCall`. With `maxing` it returns the smaller of that and `maxWait - timeSinceLastInvoke`.
- **`trailingEdge`** (10475-10485) clears the timer. It calls `func` only if `trailing && lastArgs`. `lastArgs` is cleared by every `invokeFunc`, so a single call is not invoked twice.
- **`invokeFunc`** (10426-10434) applies `func` with the last `this` and arguments, and stores `lastInvokeTime` and `result`.
- **`cancel`** (10487-10493) clears the timer and resets the state.
- **`flush`** (10495-10497) runs `trailingEdge` immediately if a timer is pending. Otherwise it returns the last `result`.

**What `maxWait` does**
Without `maxWait`, constant calls keep pushing the trailing call back, so `func` may never run. With `maxWait`, the timer is shortened to `maxWait - timeSinceLastInvoke` (10450-10451). Once `maxWait` ms have passed since the last invocation, `shouldInvoke` is true and `func` runs, even though calls are still arriving.