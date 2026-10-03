`debounce` calls `func` from a timer that re-checks `shouldInvoke` when it fires. Calling `debounced()` also calls `func` right away if the `maxWait` limit has been hit and a timer is already running. All lines below are in `lodash.js` at the pinned commit.

**Setup (10403–10421)**
- `wait` is coerced with `toNumber(wait) || 0`.
- Defaults are `leading=false` and `trailing=true`.
- If `options` is an object, `leading` and `trailing` are read from it.
- `maxing = 'maxWait' in options`, so `maxWait` counts as set whenever the key is present.
- `maxWait = nativeMax(toNumber(options.maxWait) || 0, wait)`, so it is never smaller than `wait` (10418–10419).

**Decision rule: `shouldInvoke(time)` (10455–10464)**

It returns true if any of these holds:
- It is the first call (`lastCallTime === undefined`).
- `time - lastCallTime >= wait`, meaning the calls have been quiet for `wait` ms.
- `time - lastCallTime < 0`, meaning the clock went backwards.
- `maxing && time - lastInvokeTime >= maxWait`.

**Flow**

1. **`debounced()` (10499–10518)**
   - It computes `isInvoking = shouldInvoke(now)`.
   - It stores `lastArgs`, `lastThis` and `lastCallTime`.
   - If `isInvoking` is true and no timer is running, it calls `leadingEdge`.
   - Otherwise, if no timer is running, it starts `setTimeout(timerExpired, wait)`.
   - It returns the last `result`.
2. **`leadingEdge` (10435–10441)**
   - It sets `lastInvokeTime = time` and starts the `wait` timer.
   - It calls `func` immediately only if `leading` is true. Otherwise it returns the previous `result`.
3. **`timerExpired` (10466–10473)**
   - If `shouldInvoke(now)` is true, it runs `trailingEdge`.
   - Otherwise it re-arms the timer with `remainingWait(time)`.
4. **`trailingEdge` (10475–10484)**
   - It clears `timerId`.
   - It calls `func` only if `trailing` is true and there are pending `lastArgs`. That means at least one call has happened since the last invocation.
   - Otherwise it drops the pending args.
5. **`invokeFunc` (10427–10433)**
   - It applies `func` with the saved `this` and args.
   - It clears the saved args, sets `lastInvokeTime`, and stores `result`.

**What `maxWait` does**
- It caps how long `func` can be delayed by a continuous stream of calls.
- Without it, each call pushes the trailing edge out, because `timeSinceLastCall` keeps resetting, and `func` may never run.
- With it, `remainingWait` (10443–10453) returns `min(wait - timeSinceLastCall, maxWait - timeSinceLastInvoke)`. The timer therefore fires no later than `maxWait` after the last invocation.
- When it fires, `shouldInvoke` is true through the `maxing` clause, and `trailingEdge` invokes `func`. This works even though calls are still arriving.
- The "tight loop" branch in `debounced` (10508–10513) handles a call that arrives after `maxWait` has elapsed while a timer is still pending. If `maxing` is set, it resets the timer to `wait` and calls `func` synchronously via `invokeFunc`.
- `maxWait` is measured from `lastInvokeTime`, not from the first call.

**Other methods**
- `cancel` clears the timer and resets state (10486–10492).
- `flush` runs `trailingEdge(now())` if a timer is pending, and otherwise returns the last result (10494–10496).

**Uncertainty:** I quoted the line numbers from the 10403–10525 read, but I counted offsets within that range by hand. They may be off by a line or two.