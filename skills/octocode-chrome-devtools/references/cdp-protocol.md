# CDP protocol

Load when a custom script needs domain order, sessions, or the right method. For unfamiliar or failing methods, check `https://chromedevtools.github.io/devtools-protocol/tot/<Domain>/` or the live `http://localhost:<port>/json/protocol`.

## Order and safety

- Enable before use: Page, Runtime, Network, Log for debug; DOM before CSS; Fetch before navigation; ServiceWorker/Target before worker events.
- After `Debugger.enable`, call `Debugger.setSkipAllPauses({skip:true})`.
- Dialog guard before risky navigation: on `Page.javascriptDialogOpening` call `Page.handleJavaScriptDialog({accept:true})`.
- Child targets (iframes, workers) get a `sessionId`; pass it as the third `cdp.send` argument.

## Which method

| Need | Method |
|---|---|
| Navigate | `Page.navigate` after listeners |
| Screenshot / PDF | `Page.captureScreenshot`, `Page.printToPDF` |
| Console / exceptions | `Runtime.consoleAPICalled`, `Runtime.exceptionThrown` |
| DOM state | `Runtime.evaluate`; `DOM` for node metadata |
| Whole page structure + layout | `DOMSnapshot.captureSnapshot({computedStyles, includeDOMRects:true})` |
| Trusted input | `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`, `Input.insertText` |
| Requests | `Network.requestWillBeSent`, `responseReceived`, `loadingFailed`, `getResponseBody` |
| Cookies | `Network.getAllCookies` (metadata only); `Network.setCookies` only when approved |
| Clear state | `Network.clearBrowserCache`, `Network.clearBrowserCookies` |
| Local/session storage, IndexedDB, Cache | `Runtime.evaluate` over `localStorage`, `indexedDB.databases()`, `caches.keys()` |
| Interception | `Fetch.enable` + continue/fail/fulfill |
| Targets | `Target.getTargets`, `Target.attachToTarget`, `Target.setAutoAttach` |
| Security state | `Security.visibleSecurityStateChanged` |
| Page issues (mixed content, CORS, cookies) | `Audits.enable` + `Audits.issueAdded` |
| Emulation | `Emulation.setDeviceMetricsOverride`, `setTouchEmulationEnabled`, `setGeolocationOverride` (grant `geolocation` first), `Network.setUserAgentOverride`, `Network.emulateNetworkConditions` |
| Accessibility | `Accessibility.getFullAXTree` with bounded depth |
| Heap | `HeapProfiler.takeHeapSnapshot` |

Next: protocol errors → `recovery.md` (Protocol table).
