# CDP Environment And Runtime Intent Details

## emulate

**Trigger phrases:** "mobile", "emulate device", "throttle network", "slow 3G", "offline", "fake location", "geolocation", "tablet", "responsive", "test on mobile", "iPhone", "Android", "viewport", "mobile UA"

**Purpose:** Override browser environment — device viewport, touch events, UA, network speed, geolocation — before running any other intent. All overrides are script-level and take effect before navigation.

**Domains:** No `enable` call needed for `Emulation.*`. For network throttling: `Network.enable` first. For geolocation: `Browser.grantPermissions` first.


### Two-level emulation model

| Level | How | Controls |
|---|---|---|
| **Launch-level** | `open-browser.mjs --windowSize 390x844 --userAgent "<mobile-ua>"` | Window size, launch-time UA |
| **Script-level** | `Emulation.setDeviceMetricsOverride` + `setUserAgentOverride` + `setTouchEmulationEnabled` | Viewport, DPR, mobile mode, UA, touch, Sec-CH-UA hints |

**Prefer script-level emulation for accuracy** — it gives real mobile layout (media queries fire, `window.innerWidth` matches), real device pixel ratio, real touch events, and full UA hint spoofing. Launch-level `--windowSize` is useful for initial window dimensions before CDP attaches.


### Device presets

Replace `<current-version>` / `<current-major>` with the installed Chrome version before running a copied Android preset.

| Device | Width | Height | DPR | User-Agent |
|---|---|---|---|---|
| iPhone 15 Pro | 393 | 852 | 3 | `Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1` |
| iPhone 13 | 390 | 844 | 3 | `Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1` |
| Pixel 7 (Android) | 412 | 915 | 2.625 | `Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/<current-version>.0.0.0 Mobile Safari/537.36` |
| Samsung Galaxy S23 | 360 | 780 | 3 | `Mozilla/5.0 (Linux; Android 13; SM-S911B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/<current-version>.0.0.0 Mobile Safari/537.36` |
| iPad Air (M2) | 820 | 1180 | 2 | `Mozilla/5.0 (iPad; CPU OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1` |
| Desktop 1920 | 1920 | 1080 | 1 | _(use open-browser.mjs default — desktop UA)_ |



**Adaptive emulation recipe:**

Use launch flags for initial window shape only. Use script-level `Emulation.*` before navigation for viewport, DPR, touch, UA, and locale behavior. Replace UA/version hints with the installed browser version when spoofing Chrome.

```js
const device = { width: 393, height: 852, dpr: 3, mobile: true };
await cdp.send('Page.enable', {});
await cdp.send('Network.enable', {});
await cdp.send('Emulation.setDeviceMetricsOverride', {
  width: device.width, height: device.height,
  deviceScaleFactor: device.dpr, mobile: device.mobile,
});
await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 });
// Optional: setUserAgentOverride with current, realistic UA + hints.
await cdp.send('Page.navigate', { url: TARGET_URL });
```

After load, verify `window.innerWidth`, `devicePixelRatio`, and horizontal overflow. Feature-detect browser APIs inside the page when emulating capabilities, not just viewport.

**Network throttling snippets:**
```js
await cdp.send('Network.enable', {});

// Slow 3G
await cdp.send('Network.emulateNetworkConditions', {
  offline: false, downloadThroughput: 50_000, uploadThroughput: 20_000, latency: 400,
});
console.log('[EMULATE] network: Slow 3G (50kb/s down, 400ms latency)');

// Fast 3G
await cdp.send('Network.emulateNetworkConditions', {
  offline: false, downloadThroughput: 180_000, uploadThroughput: 84_000, latency: 100,
});

// Offline
await cdp.send('Network.emulateNetworkConditions', {
  offline: true, downloadThroughput: 0, uploadThroughput: 0, latency: 0,
});

// Reset
await cdp.send('Network.emulateNetworkConditions', {
  offline: false, downloadThroughput: -1, uploadThroughput: -1, latency: 0,
});
```

**Geolocation:**
```js
await cdp.send('Browser.grantPermissions', { permissions: ['geolocation'] });
await cdp.send('Emulation.setGeolocationOverride', { latitude: 40.7128, longitude: -74.0060, accuracy: 100 });
console.log('[EMULATE] geolocation: New York City');
```

**Dark mode / media queries:**
```js
await cdp.send('Emulation.setEmulatedMedia', {
  features: [{ name: 'prefers-color-scheme', value: 'dark' }],
});
```

**Output prefixes:** `[EMULATE]` `[METRIC]` `[FINDING]`

**`[FINDING]` conditions:**
- Layout breaks at mobile viewport → `[FINDING] LAYOUT_BREAK: horizontal scroll or overflow at ${width}px`
- Network throttle reveals slow resource → `[FINDING] SLOW_RESOURCE: ${url} took ${ms}ms on throttled connection`
- Offline triggers uncaught exception → `[FINDING] NO_OFFLINE_HANDLING: app crashes when offline`

**Ordering constraint:** `Emulation.*` overrides take effect on the next navigation. Call them before `Page.navigate` — media queries and viewport-dependent layout fire during the first parse, not on DOMContentLoaded.

**Combine with:** `screenshot` (capture mobile layout), `performance` (measure on throttled network), `debug` (find mobile-only errors), `scrape` (scrape mobile-rendered content).

## inject

**Trigger phrases:** "inject script", "patch before load", "hook function", "override", "monkey-patch", "add script to page", "intercept before", "modify behavior", "bypass CSP", "add tracking"

**Purpose:** Inject JavaScript into every new document before any page script runs. Use for hooking functions, overriding globals, adding instrumentation, or bypassing checks.

**Domains:** `Page.enable`

**Key methods:**
- `Page.addScriptToEvaluateOnNewDocument({source})` → runs before page JS on every navigation
- `Page.removeScriptToEvaluateOnNewDocument({identifier})` → remove when done
- `Page.setBypassCSP({enabled:true})` → bypass Content-Security-Policy (required before injection on CSP-protected pages)

**Injection patterns:**
```js
// Hook fetch to log all requests + bodies
const { identifier } = await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
  source: `
    const _fetch = window.fetch;
    window.fetch = async function(...args) {
      const [url, init] = args;
      console.log('[INJECTED:FETCH]', typeof url === 'string' ? url : url.url, init?.body?.slice?.(0,200));
      return _fetch.apply(this, args);
    };
  `
});
console.log('[INJECT] fetch hook installed, identifier:', identifier);

// Hook XMLHttpRequest
await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
  source: `
    const _open = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url) {
      this._xhrUrl = url; this._xhrMethod = method;
      console.log('[INJECTED:XHR]', method, url);
      return _open.apply(this, arguments);
    };
  `
});

// Expose a debug channel back to CDP
await cdp.send('Runtime.addBinding', { name: '__cdpLog' });
// then cdp.on('Runtime.bindingCalled', ({name, payload}) => ...) to receive messages from injected code

// Override a specific function (e.g. disable analytics)
await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
  source: `Object.defineProperty(window, 'analytics', { get: () => ({ track: () => {}, page: () => {} }) });`
});

// Bypass CSP before injection
await cdp.send('Page.setBypassCSP', { enabled: true });
```

**Cleanup:** Remove injected scripts after the session — they persist across navigations until removed.
```js
await cdp.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
```

**Output prefixes:** `[INJECT]` `[FINDING]`

**`[FINDING]` conditions:**
- Injected hook never fires → `[FINDING] INJECT_NO_CALLS: hook installed but never triggered`
- Page detects injection and shows error → `[FINDING] INJECTION_DETECTED: page responded to override`

**Combine with:** `debug` (observe injected logs), `network` (correlate injected fetch logs with CDP network events), `security` (hook crypto APIs to observe key usage).

## monitor

**Trigger phrases:** "monitor", "watch", "keep watching", "check every N seconds", "poll", "alert me when", "watch for changes", "continuous", "long-running"

**Purpose:** Long-running observation loop — poll a page or condition repeatedly and emit findings when state changes. Useful for catching intermittent errors, watching a live dashboard, or waiting for an event.

**Domains:** `Network.enable`, `Runtime.enable`, `Log.enable` (others as needed)


**Adaptive monitor recipe:**

Monitor only the condition the user cares about. Use event listeners for async errors and one small `Runtime.evaluate` snapshot per interval.

```js
export async function run(cdp) {
  const INTERVAL_MS = 5000;
  const DURATION_MS = 60000;
  await cdp.send('Runtime.enable', {});
  await cdp.send('Network.enable', {});
  await cdp.send('Log.enable', {});

  const queue = [];
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => queue.push(exceptionDetails.text));
  cdp.on('Network.responseReceived', ({ response }) => {
    if (response.status >= 400) queue.push(`HTTP ${response.status} ${response.url}`);
  });

  for (const end = Date.now() + DURATION_MS; Date.now() < end;) {
    const { result } = await cdp.send('Runtime.evaluate', {
      expression: `JSON.stringify({ url: location.href, title: document.title,
        errorEls: document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length })`,
      returnByValue: true,
    });
    console.log(`[MONITOR] ${result.value}`);
    while (queue.length) console.log(`[FINDING] MONITOR_EVENT: ${queue.shift()}`);
    await new Promise(r => setTimeout(r, INTERVAL_MS));
  }
}
```

Write a JSON log to `cdp.outputDir` only if the user needs an artifact. Increase duration or add screenshot/network details only after the first run shows they are useful.

**Output prefixes:** `[MONITOR]` `[FINDING]` `[METRIC]` `[ACTION]`

**`[FINDING]` conditions:**
- Error count increases between iterations → `[FINDING] MONITOR_ERROR: new error at ${elapsed}s`
- URL changes unexpectedly → `[FINDING] MONITOR_REDIRECT: URL changed to ${url} at ${elapsed}s`
- DOM error indicators appear → `[FINDING] MONITOR_DOM_ERROR: error elements appeared at ${elapsed}s`
- Page becomes unresponsive (evaluate times out) → `[FINDING] MONITOR_HANG: page unresponsive at ${elapsed}s`
- Network 4xx/5xx mid-session → `[FINDING] MONITOR_NETWORK_ERROR: HTTP ${status} at ${elapsed}s`

**Agent loop usage:**

```
REASON  → what condition am I watching for? how long?
MONITOR → set TARGET_URL + DURATION_MS, run script → read [MONITOR] + [FINDING] lines
OBSERVE → check === OBSERVE === block: changes, errors, redirects
ACT     → follow [ACTION] lines: run debug on affected URL, inspect DOM errors
SAVE    → change log auto-saved to cdp.outputDir/monitor-log.json
REPEAT  → if condition not yet triggered, increase DURATION_MS and re-run
```

Stop when: target condition observed OR `[ACTION] page is stable` with zero errors.

**Combine with:** `debug` (deep-dive on errors found), `screenshot` (capture state at each change), `network` (add network tracking).
