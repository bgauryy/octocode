`debounce` calls `func` on a timer. Each call to the debounced function records its args, `this` and call time, and a single `setTimeout` is re-checked against the clock whenever it fires. `maxWait` caps how long continuous calls can postpone `func`. All lines below are in `lodash.js` at the pinned commit. I read the source only and did not run it.

**Options (10403–10425)**
- `wait` is coerced with `toNumber(wait) || 0` (10416).
- `leading` is `!!options.leading` (10418). It defaults to `false`.
- `trailing` defaults to `true` and is set from `options.trailing` if that key is present (10420).
- `maxing` is true if the key `'maxWait'` is present in `options` (10419).
- When `maxing` is set, `maxWait = max(toNumber(options.maxWait) || 0, wait)` (10420). So `maxWait` can never be less than `wait`.

**When `func` is called**
- **Decision rule.** `shouldInvoke(time)` (10455–10463) returns true if any of these hold:
  - this is the first call (`lastCallTime === undefined`);
  - `timeSinceLastCall >= wait`;
  - the clock went backwards (`timeSinceLastCall < 0`);
  - `maxing && timeSinceLastInvoke >= maxWait`.
- **Each call.** `debounced()` (10499–10521) computes `isInvoking = shouldInvoke(now)` and stores `lastArgs`, `lastThis` and `lastCallTime`.
  - If `isInvoking` is true and there is no timer, it runs `leadingEdge`. That sets `lastInvokeTime`, starts a `wait` timer, and invokes `func` only if `leading` is true (10431–10437).
  - If `isInvoking` is true and a timer exists and `maxing` is set, it restarts the timer and invokes `func` immediately (10508–10513). This is the "tight loop" case.
  - Otherwise it starts a timer if none exists and returns the last `result`.
- **Timer expiry.** `timerExpired` (10465–10471) re-evaluates `shouldInvoke`.
  - If true, it runs `trailingEdge`.
  - If false, it re-arms the timer for `remainingWait(time)`.
- **`remainingWait`** (10439–10447) is `wait - timeSinceLastCall`. When `maxing`, it is the smaller of that and `maxWait - timeSinceLastInvoke`.
- **Trailing edge.** `trailingEdge` (10473–10482) clears `timerId`. It invokes `func` only if `trailing` is true and `lastArgs` is set, meaning there was a call since the last invocation. Otherwise it drops the args and returns the previous `result`.

**What `maxWait` does**
- Without `maxWait`, a steady stream of calls spaced less than `wait` apart keeps pushing the trailing call back indefinitely.
- With `maxWait`, `func` is forced to run once `maxWait` ms have passed since the last invocation, even if calls keep arriving. The check is in `shouldInvoke` (10462) and the timer shortening is in `remainingWait` (10444–10446).
- `lastInvokeTime` is reset by every invocation and by `leadingEdge` (10433), so the `maxWait` window restarts from there.

**Other methods**
- `cancel` clears the timer and all state (10484–10490).
- `flush` runs `trailingEdge` immediately if a timer is pending, otherwise it returns the last `result` (10492–10494).

**Uncertainty:** the line numbers are approximate for the inner helpers. I read the function body as one block starting at 10403 and counted down from there. The `function debounce` line at 10403 and the `debounced` declaration at 10499 come directly from grep.