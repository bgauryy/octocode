# CDP Domain Map

Load when unsure which Chrome DevTools Protocol domain/method to use. Why: choose exact protocol calls instead of guessing.

| Need | CDP domain / method |
|---|---|
| navigate/load evidence | `Page.enable`, listeners, then `Page.navigate` |
| screenshots/PDF | `Page.captureScreenshot`, `Page.printToPDF` |
| console/exceptions | `Runtime.enable`, `Runtime.consoleAPICalled`, `Runtime.exceptionThrown` |
| DOM state/selectors | `Runtime.evaluate`; use `DOM` for node metadata, not giant dumps |
| actionability | `Runtime.evaluate` + `DOM` + `Accessibility`; use `Input.*` for trusted actions |
| mouse/keyboard/scroll | `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`, `Input.insertText` |
| HTTP requests | `Network.requestWillBeSent`, `Network.responseReceived`, `Network.loadingFailed` |
| response body | call `Network.getResponseBody` immediately after `responseReceived` |
| cookies metadata | `Network.getAllCookies`, `Network.getCookies`; never print values by default |
| cookie injection | `Network.setCookies` only with explicit approval |
| clear browser cookies/cache | `Network.clearBrowserCookies`, `Network.clearBrowserCache` |
| local/session storage | `Runtime.evaluate` over `localStorage` / `sessionStorage` keys |
| IndexedDB names | `Runtime.evaluate` with `indexedDB.databases()` |
| Cache API names | `Runtime.evaluate` with `caches.keys()` |
| service workers | `ServiceWorker.*` or `navigator.serviceWorker.getRegistrations()` |
| request interception | `Fetch.enable` before navigation; then continue/fail/fulfill requests |
| tab/session discovery | `Target.getTargets`, `Target.attachToTarget` |
| security state | `Security.visibleSecurityStateChanged` event |
| device/geo/network emulation | `Emulation.setDeviceMetricsOverride`, `Emulation.setTouchEmulationEnabled`, `Emulation.setGeolocationOverride` (call `Browser.grantPermissions({permissions:['geolocation']})` first), `Network.setUserAgentOverride`, `Network.emulateNetworkConditions` |
| perf metrics | `Performance.*` + browser `performance` APIs |
| a11y | `Accessibility.getFullAXTree` with bounded depth |
| structured page issues (quirks mode, mixed content, deprecations, cookie/CORS problems) | `Audits.enable` + `Audits.issueAdded` — more reliable than scraping console text |
| full DOM+style+layout in one call | `DOMSnapshot.captureSnapshot({computedStyles, includeDOMRects: true})` — a whole-page structural extract, much cheaper than looping `Runtime.evaluate` + `getBoundingClientRect` per element; does not replace per-element stability polling when reflow timing matters |
| heap snapshot for leak investigation | `HeapProfiler.enable` + `HeapProfiler.takeHeapSnapshot`; collect chunks via `HeapProfiler.addHeapSnapshotChunk` |

If a method is unavailable, load `references/recovery.md`; Chrome CDP changes across versions.
