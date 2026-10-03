`debounce` (`lodash.js:10403`) calls `func` on a timer, at the leading edge, the trailing edge, or when `maxWait` forces it. The decision logic is `shouldInvoke` plus the timer callbacks. Line numbers below are computed from the function start at 10403, so they are approximate.

**Setup (10403–10421)**
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to false and `trailing` defaults to true. Options override them: `leading = !!options.leading`, and `trailing` is read only if `'trailing' in options`.
- `maxing = 'maxWait' in options`. Then `maxWait = nativeMax(toNumber(options.maxWait) || 0, wait)`, so `maxWait` can never be smaller than `wait`.

**The decision: `shouldInvoke(time)` (about 10449–10458)**
It returns true if any of these holds:
- There has been no previous call (`lastCallTime === undefined`).
- The quiet period has elapsed (`timeSinceLastCall >= wait`).
- The clock went backwards (`timeSinceLastCall < 0`).
- `maxing && timeSinceLastInvoke >= maxWait`.

**Flow**
- **Calling `debounced()` (about 10499):**
  - It records `lastArgs`, `lastThis` and `lastCallTime`.
  - If `shouldInvoke` is true and no timer is running, it calls `leadingEdge`. That sets `lastInvokeTime` and starts `setTimeout(timerExpired, wait)`. It invokes `func` immediately only if `leading` is true; otherwise it returns the previous `result`.
  - If `shouldInvoke` is true, a timer is already running, and `maxing` is set (the "tight loop" case), it restarts the timer and invokes `func` right away.
  - Otherwise it starts a timer if none is running and returns the last `result`.
- **`timerExpired` (about 10460):** It re-checks `shouldInvoke`. If true, it calls `trailingEdge`. If not, it reschedules with `remainingWait(time)`.
- **`trailingEdge` (about 10469):** It clears `timerId` and invokes `func` only if `trailing && lastArgs`. `lastArgs` is set only when `debounced` was called since the last invocation, so `func` is never called twice for the same call.
- **`remainingWait` (about 10441):** It returns `wait - timeSinceLastCall`. When `maxing`, it returns the smaller of that and `maxWait - timeSinceLastInvoke`.

**What `maxWait` does:** Without it, continuous calls keep pushing the trailing call back, because each call resets the quiet-period check, so `func` never runs while calls keep arriving. With it, `func` is guaranteed to run at least once every `maxWait` ms. The `maxing && timeSinceLastInvoke >= maxWait` clause in `shouldInvoke` and the capped `remainingWait` are what enforce this. `lastInvokeTime` is reset on each invocation.

**Helpers:**
- `cancel` clears the timer and all state.
- `flush` runs `trailingEdge` immediately if a timer is pending.

I read the source directly, so there is no uncertainty about the behavior. Only the line numbers are approximate, except the function start at 10403 and `debounced` at 10499, which the search returned.