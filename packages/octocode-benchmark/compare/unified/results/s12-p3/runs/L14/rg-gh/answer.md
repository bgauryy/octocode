`debounce` calls `func` on the trailing edge by default: after `wait` ms have passed with no new calls. `leading: true` also calls it on the first call of a burst. `maxWait` caps how long continuous calls can postpone `func`. All line numbers below are in `lodash.js`. I read the code but did not run it.

**Options (`lodash.js:10416-10424`)**
- `wait` is converted with `toNumber(wait) || 0`.
- `leading` defaults to false, and `trailing` defaults to true.
- `maxing = 'maxWait' in options`. This is a key-presence check, so `maxWait: undefined` still turns it on.
- `maxWait = max(toNumber(options.maxWait) || 0, wait)`, so it is never smaller than `wait`.

**Decision logic**
- **`shouldInvoke(time)` (10453-10462)** returns true in four cases:
  - It is the first call (`lastCallTime === undefined`).
  - `time - lastCallTime >= wait`, meaning the calls have gone quiet.
  - `time - lastCallTime < 0`, meaning the clock went backwards.
  - `maxing && time - lastInvokeTime >= maxWait`.
- **`debounced()` (10499-10521)** records `lastArgs`, `lastThis` and `lastCallTime`, then checks `shouldInvoke`.
  - If it is true and there is no timer, it calls `leadingEdge`. That sets `lastInvokeTime`, starts a `wait` timer, and calls `func` only if `leading` is true.
  - If it is true, a timer already exists and `maxing` is set, it restarts the timer and calls `func` immediately (10510-10515). The comment there says this handles invocations in a tight loop.
  - Otherwise it starts a timer if none is running and returns the last `result`.
- **`timerExpired` (10464-10470)** re-checks `shouldInvoke`. If true, it runs `trailingEdge`. If not, it reschedules with `remainingWait`.
- **`remainingWait` (10445-10451)** returns `wait - timeSinceLastCall`. With `maxing`, it returns the smaller of that and `maxWait - timeSinceLastInvoke`.
- **`trailingEdge` (10472-10481)** calls `func` only if `trailing` is true and `lastArgs` is set. `lastArgs` is cleared after each invocation, so a leading-only call is not repeated at the trailing edge.

**What `maxWait` does:** continuous calls normally keep resetting the `wait` timer, so `func` could be delayed forever. With `maxWait`, `func` is forced to run once `maxWait` ms have passed since the last invocation. This works through `remainingWait`, which shortens the timer, and the `maxing` clause in `shouldInvoke`. It behaves like a throttle ceiling.

**Other methods**
- `cancel` (10483-10490) clears the timer and resets all state.
- `flush` (10492-10494) runs `trailingEdge` immediately if a timer is pending. Otherwise it returns the last `result`.

**Uncertainty:** I couldn't confirm the checkout is at 2b5e6f7399, because `git rev-parse` failed in the sandbox. The line numbers come from the working tree you gave me.