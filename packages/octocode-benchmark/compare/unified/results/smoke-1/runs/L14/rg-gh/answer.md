**Short answer:** `debounce` calls `func` on the trailing edge by default. That is when `wait` ms have passed since the last call to the debounced function. `leading: true` also calls it on the first call. `maxWait` caps how long a steady stream of calls can delay it. All of this is in `lodash.js:10403-10526`. `dist/lodash.js` has the same code at the same lines.

**Options** (`lodash.js:10418-10424`)
- `wait` is coerced to a number, defaulting to 0.
- `leading` defaults to false, and `trailing` defaults to true.
- `maxing` is true if the key `'maxWait'` is present in `options`. `maxWait` is then `max(toNumber(options.maxWait) || 0, wait)`, so it can never be less than `wait`.

**Decision logic**
- **`shouldInvoke(time)`** (`:10455-10464`) is true if any of these hold:
  - it is the first call (`lastCallTime === undefined`);
  - `timeSinceLastCall >= wait`;
  - `timeSinceLastCall < 0` (the clock went backwards);
  - `maxing && timeSinceLastInvoke >= maxWait`.
- **Each call to `debounced()`** (`:10499-10522`) does the following:
  - It evaluates `shouldInvoke`, then records `lastArgs`, `lastThis` and `lastCallTime`.
  - If `shouldInvoke` is true and no timer is running, it runs `leadingEdge`.
  - `leadingEdge` (`:10436-10443`) sets `lastInvokeTime`, starts a `wait` timer, and calls `func` immediately only if `leading` is true. Otherwise it returns the previous `result`.
  - If `shouldInvoke` is true, a timer is running and `maxing` is set, it restarts the timer and invokes `func` immediately (`:10511-10516`). The comment there says this handles invocations in a tight loop.
  - Otherwise, if no timer is running, it starts a `wait` timer. It returns the last `result` in all these non-invoking cases.
- **`timerExpired`** (`:10466-10473`) re-checks `shouldInvoke`.
  - If true, it runs `trailingEdge`.
  - If false, it reschedules the timer for `remainingWait(time)`.
- **`remainingWait`** (`:10445-10453`) is `wait - timeSinceLastCall`. When `maxing` is set, it is the smaller of that value and `maxWait - timeSinceLastInvoke`.
- **`trailingEdge`** (`:10475-10485`) clears `timerId`. It invokes `func` only if `trailing` is true and `lastArgs` is set, meaning there was at least one call since the last invocation. Otherwise it clears the saved args and returns `result`.
- **`flush`** (`:10495-10497`) runs `trailingEdge` immediately if a timer is pending. `cancel` (`:10487-10493`) clears the timer and all state.

**What `maxWait` does:** without it, a call every less-than-`wait` ms keeps pushing the trailing call back forever. With it, `shouldInvoke` also becomes true once `maxWait` ms have passed since the last invocation (`:10463`). `remainingWait` shortens the timer so it fires at that point (`:10451`). So `func` runs at least once per `maxWait` ms while calls continue.

**Uncertainty:** none on the logic, which I read directly. The JSDoc above the function was not read.