`debounce` calls `func` on a timer-driven trailing edge, optionally on the leading edge, and `maxWait` caps how long a stream of calls can keep postponing it. Everything below is in `lodash.js`, `debounce` at lines ~10403–10526. I read the source only and did not run it.

**Setup (~10403–10420)**
- `wait` is coerced with `toNumber(wait) || 0`.
- `leading` defaults to `false` and `trailing` to `true`.
- `maxing = 'maxWait' in options`. If it is set, `maxWait = nativeMax(toNumber(options.maxWait) || 0, wait)`, so `maxWait` is never less than `wait`.

**Deciding to invoke (`shouldInvoke`, ~10451)**
It returns true when any of these holds:
- `lastCallTime === undefined`, meaning this is the first call.
- `timeSinceLastCall >= wait`, meaning calls have been quiet for `wait` ms.
- `timeSinceLastCall < 0`, meaning the system clock went backwards.
- `maxing && timeSinceLastInvoke >= maxWait`, meaning `maxWait` has elapsed since the last invocation.

**Each call to `debounced` (10500–10522)**
- It records `lastArgs`, `lastThis` and `lastCallTime`, so `func` always receives the latest arguments.
- If `shouldInvoke` is true and no timer is running, it runs `leadingEdge`. That sets `lastInvokeTime`, starts `setTimeout(timerExpired, wait)`, and calls `func` immediately only if `leading` is true.
- If `shouldInvoke` is true, a timer is already running, and `maxing` is set, it restarts the timer and calls `func` immediately via `invokeFunc` (10511–10515). The code comments this as handling invocations in a tight loop.
- Otherwise, if there is no timer, it starts one for `wait` ms.
- It returns the last result.

**Timer expiry (`timerExpired`, ~10464)**
- If `shouldInvoke(now())` is true, it runs `trailingEdge`.
- If not, it reschedules the timer for `remainingWait(time)`.
- `remainingWait` is `wait - timeSinceLastCall`. With `maxing`, it is the smaller of that and `maxWait - timeSinceLastInvoke`.

**Trailing edge (`trailingEdge`, ~10472)**
It clears `timerId`. It calls `func` only if `trailing && lastArgs`. `lastArgs` is cleared by every `invokeFunc`, so a leading-only invocation with no further calls does not fire again. Otherwise it clears the saved arguments and returns the last result.

**What `maxWait` does**
- Without `maxWait`, every call resets the quiet period, so continuous calls can postpone `func` indefinitely.
- With `maxWait`, `func` is forced to run at least once per `maxWait` ms during continuous calling. The timer wakes up at the earlier of the quiet-period end and the `maxWait` deadline.
- The next call after the deadline then also triggers an invocation.

**Other methods**
- `cancel` clears the timer and all saved state.
- `flush` calls `trailingEdge(now())` if a timer is pending, and otherwise returns the last result.

I did not check `dist/lodash.js` or the npm modular builds. They match this definition by name only (the search found `function debounce` there too).