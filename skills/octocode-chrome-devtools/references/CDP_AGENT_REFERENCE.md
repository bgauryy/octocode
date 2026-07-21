# Chrome DevTools Protocol (CDP) — Agent Reference

> Use `cdp-runner.mjs` / `cdp-sandbox.mjs` — connection is handled automatically. This reference covers domain APIs, method params, events, and error codes only.

## Source of Truth — Actual CDP API

Use this file for common workflows and known gotchas. Use the official protocol pages for the exact current API shape:

```text
https://chromedevtools.github.io/devtools-protocol/tot/<Domain>/
```

Example: `https://chromedevtools.github.io/devtools-protocol/tot/DOM/`

Before using any unfamiliar method, optional parameter, return field, event, or experimental/deprecated API, check the matching domain page. If Chrome is already running with remote debugging and the task depends on the browser's exact supported protocol, fetch the local protocol description instead:

```bash
curl -fsS "http://127.0.0.1:9222/json/protocol" > "$TMPDIR/cdp-protocol.json"
rg -n '"domain" *: *"DOM"|"name" *: *"getDocument"' "$TMPDIR/cdp-protocol.json"
```

Replace `9222` with the active CDP port if needed. Tip-of-tree docs expose the full protocol surface and can move ahead of a user's installed Chrome. The local `/json/protocol` endpoint is the best authority for that running browser.

For browser/Web APIs invoked through `Runtime.evaluate`, this CDP reference is only illustrative. Verify current Web API documentation and feature-detect in the target page (`'storage' in navigator`, `'PerformanceObserver' in window`, etc.) before relying on optional or fast-moving APIs.


## 0. Domain Enable Map

Enable only what you need. Call at the **top** of `run()` before any listener or send.

| User request | Domains to enable |
|---|---|
| network requests / errors | `Network.enable` |
| console / exceptions | `Runtime.enable`, `Log.enable` |
| performance metrics | `Performance.enable` |
| DOM queries / rendering | `DOM.enable`, `Runtime.enable` |
| CSS / computed styles | `DOM.enable`, `CSS.enable` (**DOM first**) |
| screenshots / navigation | `Page.enable` |
| accessibility tree | `Accessibility.enable` |
| heap snapshot / memory leak | `HeapProfiler.enable` |
| file upload | `DOM.enable`, `Runtime.enable` → `DOM.setFileInputFiles(nodeId, files:[…])` → dispatch `change`/`input` events |
| wait for network idle | `Network.enable` → track `requestWillBeSent`/`loadingFinished` pairs |
| wait for selector with actionability | `DOM.enable`, `Runtime.enable` → poll `getBoundingClientRect` + `getComputedStyle` |
| shadow DOM query / click | `DOM.enable`, `Runtime.enable` → `DOM.getDocument({pierce:true})`; `DOM.querySelector` does **NOT** cross shadow roots |
| fetch mocking / intercept | `Fetch.enable` with `patterns:[{urlPattern,requestStage}]` (no zero-arg form) |
| JS function coverage | `Profiler.enable` → `Profiler.startPreciseCoverage` |
| CSS rule coverage | `DOM.enable`, `CSS.enable` → `CSS.startRuleUsageTracking` |
| source map resolution | `Debugger.enable` + `Debugger.setSkipAllPauses({skip:true})` → `Debugger.scriptParsed`; use `createSourceMapResolver(cdp)` from `scripts/sourcemap-resolver.mjs` |
| security events | `Security.enable` → listen for `Security.visibleSecurityStateChanged` |
| tracing / timeline | `Page.enable` (Tracing needs no enable call) |
| mobile / viewport / touch | no enable — `Emulation.setDeviceMetricsOverride` + `setTouchEmulationEnabled` + `setUserAgentOverride` (call **before** navigate) |
| emulation / geolocation | no enable — call `Emulation.*` directly; geolocation also needs `Browser.grantPermissions` |
| dark mode / media queries | no enable — `Emulation.setEmulatedMedia` |
| network throttling / offline | `Network.enable` → `Network.emulateNetworkConditions` |
| automate / login / clicks | `Page.enable`, `Runtime.enable`, `Network.enable` |
| websocket surveillance | `Network.enable` |
| security audit (full) | `Network.enable`, `Runtime.enable`, `DOM.enable`, `Page.enable` |
| all / full audit | all of the above |

### Event ordering — non-negotiable
Enable domains → attach `cdp.on(...)` → navigate / evaluate. **Events fire and are gone** — a listener registered after navigation misses prior events silently.

### Debugger gotcha
When `Debugger.enable` is active, any `debugger` statement blocks all `Runtime.evaluate` calls indefinitely. Add immediately after:
```js
await cdp.send('Debugger.enable', {});
await cdp.send('Debugger.setSkipAllPauses', { skip: true });
```

### Dialog guard
Add when navigating pages that may open `alert()`/`confirm()`/`prompt()`:
```js
cdp.on('Page.javascriptDialogOpening', () =>
  cdp.send('Page.handleJavaScriptDialog', { accept: true }));
```

### Output prefixes (quick list)
`[FINDING]` `[ACTION]` `[METRIC]` `[NETWORK]` `[NETWORK_ERROR]` `[NETWORK_FAILED]` `[EXCEPTION]` `[EXCEPTION_LOCATION]` `[CONSOLE:TYPE]` `[LOG:LEVEL]` `[PERFORMANCE]` `[DOM]` `[CSS]` `[SECURITY]` `[SCREENSHOT]` `[SCRAPE]` `[EMULATE]` `[AUTOMATE]` `[INJECT]` `[MONITOR]` `[SEARCH]` `[AUTH]` `[AUTH_COMPLETE]` `[AUTH_TIMEOUT]` `[SOURCEMAP]`
Full semantics → `INTENTS.md` → Output Prefix Reference.

---

## 1. Target — Multi-tab & context management

| Method | Input | Output | Use when |
|---|---|---|---|
| `Target.createTarget` | `url`, `width?`, `height?`, `browserContextId?` | `targetId` | Open a new tab per task |
| `Target.attachToTarget` | `targetId`, `flatten: true` | `sessionId` | Get a session; flat mode = one socket for all tabs |
| `Target.closeTarget` | `targetId` | `success` | Dispose a tab when done |
| `Target.createBrowserContext` | `disposeOnDetach?: true` | `browserContextId` | Isolated context (no shared cookies/storage) |
| `Target.disposeBrowserContext` | `browserContextId` | — | Clean up context |
| `Target.getTargets` | — | `targetInfos[]` | List open tabs (`targetId`, `url`, `type`, `attached`) |
| `Target.setAutoAttach` | `autoAttach: true`, `waitForDebuggerOnStart`, `flatten: true` | — | Auto-attach to child frames/workers |

**Events**: `targetCreated` · `targetDestroyed` · `targetCrashed` ← must restart on crash


## 2. Page — Navigation & capture

Call `Page.enable` first.

| Method | Key Input | Output | Use when |
|---|---|---|---|
| `Page.navigate` | `url` | `frameId`, `loaderId`, `errorText?` | Navigate; wait for `frameStoppedLoading` after |
| `Page.reload` | `ignoreCache?: true` | — | Refresh page |
| ~~`Page.setContent`~~ | ~~`html`, `frameId?`~~ | — | **REMOVED in Chrome 112+** — use `Page.navigate` with a `data:text/html,<html>…</html>` URL, or inject HTML via `Runtime.evaluate("document.body.innerHTML = '…'")`|
| `Page.captureScreenshot` | `format: "png"\|"jpeg"`, `quality?`, `clip?: {x,y,width,height,scale}` | `data` (base64) | Screenshot page or region |
| `Page.printToPDF` | `printBackground`, `paperWidth`, `paperHeight` | `data` (base64 PDF) | Render page as PDF |
| `Page.handleJavaScriptDialog` | `accept: bool`, `promptText?` | — | Handle `alert`/`confirm`/`prompt` |
| `Page.setBypassCSP` | `enabled: true` | — | Disable CSP so injected scripts run |
| `Page.addScriptToEvaluateOnNewDocument` | `source` (JS string) | `identifier` | Inject script that runs on **every** new page/frame before any other JS — use to remove anti-bot globals, set up stubs |
| `Page.removeScriptToEvaluateOnNewDocument` | `identifier` | — | Remove an injected script |
| `Page.createIsolatedWorld` | `frameId`, `worldName?` | `executionContextId` | Create a JS world isolated from the page's globals — safe script injection |
| `Page.getNavigationHistory` | — | `currentIndex`, `entries[]` | Read back/forward history |

**Events**: `loadEventFired` · `frameNavigated` · `frameStoppedLoading` · `javascriptDialogOpening` · `downloadWillBegin`


## 3. Runtime — JavaScript execution

Call `Runtime.enable` for events.

| Method | Key Input | Output | Use when |
|---|---|---|---|
| `Runtime.evaluate` | `expression`, `contextId?`, `returnByValue?: true`, `awaitPromise?: true`, `userGesture?: true` | `result: {type, value}` or `{objectId}` | Run JS in page; pass `contextId` to target a specific iframe |
| `Runtime.callFunctionOn` | `functionDeclaration`, `objectId`, `returnByValue` | `result` | Call function on a remote object — avoids re-fetching |
| `Runtime.getProperties` | `objectId`, `ownProperties?: true` | `result[]` | Enumerate properties of a remote object |
| `Runtime.addBinding` | `name` (string) | — | Expose `window.<name>(payload)` to page JS; agent receives `Runtime.bindingCalled` event — only way for page to call back into the agent |
| `Runtime.removeBinding` | `name` | — | Remove an exposed binding |
| `Runtime.runIfWaitingForDebugger` | — | — | Resume a target paused at startup |

**Events**: `consoleAPICalled` · `exceptionThrown` · `executionContextCreated` (fires per frame — capture `contextId` for iframe targeting) · `bindingCalled`


## 4. DOM — Document querying

Call `DOM.enable` for events.

| Method | Key Input | Output | Use when |
|---|---|---|---|
| `DOM.getDocument` | `depth?: 2`, `pierce?: false` | `root` node (nodeId, children) | Get root; `depth:-1` = full tree (expensive); `pierce:true` = include shadow roots as DOCUMENT_FRAGMENT (nodeType 11) children |
| `DOM.querySelector` | `nodeId` (root=1), `selector` | `nodeId` | First element matching CSS selector — **does NOT pierce shadow boundaries** even with `pierce:true` on `getDocument` |
| `DOM.querySelectorAll` | `nodeId`, `selector` | `nodeIds[]` | All matching elements — same shadow limitation as above |
| `DOM.getAttributes` | `nodeId` | `attributes` (flat `[name,val,…]`) | Read all attributes |
| `DOM.setAttributeValue` | `nodeId`, `name`, `value` | — | Set an attribute |
| `DOM.setOuterHTML` | `nodeId`, `outerHTML` | — | Replace a node's entire HTML |
| `DOM.setFileInputFiles` | `nodeId` (or `backendNodeId`/`objectId`), `files: string[]` | — | Set files on an `input[type="file"]` — paths must be **absolute** on the Chrome host machine; dispatch `change`/`input` events manually after for framework reactivity |
| `DOM.getBoxModel` | `nodeId` | `model.content` (8-point polygon), `width`, `height` | Bounding box for click coords: `cx=(x1+x3)/2` |
| `DOM.scrollIntoViewIfNeeded` | `nodeId` | — | Scroll element into viewport before clicking |
| `DOM.focus` | `nodeId` | — | Focus element before keyboard input |
| `DOM.resolveNode` | `nodeId` | `object.objectId` | Bridge DOM → Runtime (use with `callFunctionOn`) |


## 5. Input — Mouse, keyboard, touch

| Method | Key Input | Use when |
|---|---|---|
| `Input.dispatchMouseEvent` | `type: "mousePressed"\|"mouseReleased"\|"mouseMoved"\|"mouseWheel"`, `x`, `y`, `button: "left"`, `clickCount` | Click = `mousePressed` then `mouseReleased`. Scroll = `mouseWheel` + `deltaX`/`deltaY` |
| `Input.insertText` | `text` | Type a string into focused element — far simpler than key dispatch |
| `Input.dispatchKeyEvent` | `type: "keyDown"\|"keyUp"\|"char"`, `key`, `code`, `text?` | Special keys (Enter, Tab, Backspace, Escape) |
| `Input.dispatchTouchEvent` | `type: "touchStart"\|"touchEnd"`, `touchPoints[]` | Touch simulation when mobile emulation is on |


## 6. Network — HTTP monitoring & cookies

Call `Network.enable` first.

| Method | Key Input | Output | Use when |
|---|---|---|---|
| `Network.setExtraHTTPHeaders` | `headers: {}` | — | Inject auth/custom headers into every request |
| `Network.setCookies` | `cookies[]` (name, value, domain) | — | Set cookies before navigation |
| `Network.getCookies` | `urls[]` | `cookies[]` | Read cookies for given origins |
| `Network.clearBrowserCookies` | — | — | Wipe all cookies |
| `Network.getResponseBody` | `requestId` | `body`, `base64Encoded` | Read response body — call right after `loadingFinished` |
| `Network.setBlockedURLs` | `urls[]` (wildcards ok, e.g. `"*.analytics.com/*"`) | — | Block URL patterns without full Fetch interception overhead |
| `Network.emulateNetworkConditions` | `offline`, `latency`, `downloadThroughput`, `uploadThroughput` | — | Simulate slow/offline network |

**Events**: `requestWillBeSent` · `responseReceived` · `loadingFinished` · `loadingFailed` · `webSocketCreated`


## 7. Fetch — Request interception (modern)

Replaces `Network.setRequestInterception`. Every paused request **must** receive exactly one of: `continueRequest`, `fulfillRequest`, or `failRequest` — otherwise the page hangs.

| Method | Key Input | Use when |
|---|---|---|
| `Fetch.enable` | `patterns[]` (`urlPattern`, `requestStage: "Request"\|"Response"`) | Start intercepting |
| `Fetch.continueRequest` | `requestId`, `headers[]?`, `url?`, `method?`, `postData?` | Let through (optionally modified) |
| `Fetch.fulfillRequest` | `requestId`, `responseCode`, `responseHeaders[]`, `body` (base64) | Return a mocked response |
| `Fetch.failRequest` | `requestId`, `errorReason: "BlockedByClient"\|"Failed"\|"TimedOut"` | Abort the request |

**Event**: `Fetch.requestPaused` — fires with `requestId`, `request`, `frameId`, `resourceType`, `responseStatusCode?`


## 8. Emulation — Device & environment

| Method | Key Input | Use when |
|---|---|---|
| `Emulation.setDeviceMetricsOverride` | `width`, `height`, `deviceScaleFactor`, `mobile` | Set viewport — call before navigation for consistent layout |
| `Emulation.setUserAgentOverride` | `userAgent`, `acceptLanguage?`, `platform?` | Spoof UA string |
| `Emulation.setGeolocationOverride` | `latitude`, `longitude`, `accuracy` | Mock GPS |
| `Emulation.setTimezoneOverride` | `timezoneId` (e.g. `"America/New_York"`) | Spoof timezone |
| `Emulation.setLocaleOverride` | `locale` (e.g. `"fr-FR"`) | Change `navigator.language` |
| `Emulation.setScriptExecutionDisabled` | `value: true` | Block all JS on page |
| `Emulation.setDefaultBackgroundColorOverride` | `color: {r,g,b,a}` | `a:0` = transparent PNG screenshot |


## 9. Security — SSL / certificate handling

| Method | Key Input | Use when |
|---|---|---|
| `Security.enable` | — | Enable security events |
| `Security.setIgnoreCertificateErrors` | `ignore: true` | Ignore SSL errors — essential for local/staging HTTPS |

**Event**: `Security.visibleSecurityStateChanged` — fires when visible page security state changes (`secure`, `insecure`, `neutral`)


## 10. Debugger — JS debugging

Call `Debugger.enable` first (returns `debuggerId`).

| Method | Key Input | Output | Use when |
|---|---|---|---|
| `Debugger.setBreakpointByUrl` | `url`, `lineNumber`, `columnNumber?` | `breakpointId`, `locations[]` | Set a breakpoint |
| `Debugger.removeBreakpoint` | `breakpointId` | — | Clean up |
| `Debugger.pause` / `resume` | — | — | Force pause / resume execution |
| `Debugger.stepOver` / `stepInto` / `stepOut` | — | — | Step while paused |
| `Debugger.evaluateOnCallFrame` | `callFrameId`, `expression`, `returnByValue` | `result` | Inspect variables in paused scope |

**Events**: `Debugger.paused` (has `callFrames[]`, `reason`) · `Debugger.resumed` · `Debugger.scriptParsed`


## 11. CSS, Storage, Performance, Log, Browser

| Domain.Method | Key Input/Output | Use when |
|---|---|---|
| `CSS.enable` + `CSS.getMatchedStylesForNode` | `nodeId` → `matchedCSSRules[]`, `computedStyle[]` | Inspect which CSS rules apply |
| `CSS.getComputedStyleForNode` | `nodeId` → `[{name, value}]` | Read final computed CSS |
| `CSS.setStyleTexts` | `edits[]` (styleSheetId, range, text) | Live-edit CSS |
| `Storage.clearDataForOrigin` | `origin`, `storageTypes: "all"` | Wipe cookies + localStorage + IndexedDB |
| `Performance.enable` + `Performance.getMetrics` | → `metrics[]` | Key metrics: `JSHeapUsedSize`, `TaskDuration`, `LayoutCount`, `FirstMeaningfulPaint` |
| `Log.enable` → `Log.entryAdded` event | `{source, level, text, url, lineNumber}` | Capture all browser/console output |
| `Browser.grantPermissions` | `permissions[]`, `origin` | Pre-grant geolocation/notifications/clipboard without prompt |
| `Browser.setDownloadBehavior` | `behavior: "allow"`, `downloadPath` | Route downloads to a folder |
| `Browser.getVersion` | → `product`, `protocolVersion`, `userAgent` | Identify browser at startup |


## 12. Agent Patterns

Keep runnable code in `SCRIPT_PATTERNS.md`; use this section as the CDP behavior checklist.

| Pattern | Key CDP rule | Runnable reference |
|---|---|---|
| click element | `DOM.getBoxModel` gives an 8-point content polygon; click center with `Input.dispatchMouseEvent` | `SCRIPT_PATTERNS_ASYNC_WORKERS.md` -> `waitForSelector with Actionability` |
| fill text | focus node then `Input.insertText`, or set native input value and dispatch `input` + `change` for frameworks | `INTENTS_AUTOMATION.md` -> `automate` |
| wait for selector | CDP has no native wait; poll with `Runtime.evaluate` or `DOM.querySelector` | `SCRIPT_PATTERNS_ASYNC_WORKERS.md` |
| wait for network idle | attach `Network.*` listeners before navigation; track pending requests until quiet | `SCRIPT_PATTERNS_ASYNC_WORKERS.md` |
| iframe JS | capture `Runtime.executionContextCreated` and pass `contextId` to `Runtime.evaluate` | section 3 |
| page callback | `Runtime.addBinding` exposes `window.<name>(payload)`; listen for `Runtime.bindingCalled` | section 3 |
| pre-load inject | `Page.addScriptToEvaluateOnNewDocument` before navigation; remove by `identifier` when done | `INTENTS_ENVIRONMENT.md` -> `inject` |
| mock/block requests | `Fetch.enable` before navigation; every `requestPaused` needs continue/fulfill/fail | section 7 |
| file upload | use absolute host paths; dispatch `input` and `change` afterward | `SCRIPT_PATTERNS_BROWSER.md` -> `File Upload` |
| shadow DOM | `DOM.querySelector` never crosses shadow roots; use recursive `Runtime.evaluate`; closed roots stay inaccessible | `SCRIPT_PATTERNS_BROWSER.md` |
| isolated context | browser-level `Target.createBrowserContext` requires browser-level WebSocket; tab-level connections may reject it | `RECOVERY.md` |
| self-signed TLS | `Security.enable` then `Security.setIgnoreCertificateErrors({ ignore: true })` before navigate | section 9 |

Always handle: `Target.targetCrashed`, `Runtime.exceptionThrown`, `Log.entryAdded` with `level: "error"`, `Fetch.requestPaused` deadlocks, and `Page.navigate` responses containing `errorText`.


## 13. ServiceWorker — Lifecycle tracking

Call `ServiceWorker.enable` **before** navigation. All events are deduplicated — the same version may emit multiple identical state updates; filter by `scriptURL + status + runningStatus`.

| Method / Event | Key fields | Use when |
|---|---|---|
| `ServiceWorker.enable` | — | Start receiving SW events — must precede navigation |
| `ServiceWorker.disable` | — | Stop events (call at cleanup) |
| `ServiceWorker.skipWaiting` | `scopeURL` | Force a waiting SW to activate immediately |
| `ServiceWorker.updateRegistration` | `scopeURL` | Trigger an update check |
| `ServiceWorker.unregister` | `scopeURL` | Unregister a SW |
| Event: `workerRegistrationUpdated` | `registrations[{registrationId, scopeURL, isDeleted}]` | SW registered or unregistered |
| Event: `workerVersionUpdated` | `versions[{scriptURL, status, runningStatus}]` | Any state transition |

**`status` values (in order):** `new` → `installing` → `installed` → `activating` → `activated` → `redundant`

**`runningStatus` values:** `stopped` → `starting` → `running` → `stopping`

**Filter noise:** `workerRegistrationUpdated` fires on enable for every SW already registered in the browser (including extensions). Filter `chrome-extension:` scopes. Deduplicate `workerVersionUpdated` on `scriptURL + status/runningStatus`.

→ **Runnable implementation:** `SCRIPT_PATTERNS.md → Service Worker Lifecycle`


## 14. Target — Worker & multi-context sessions

`Target` domain manages tabs, workers, and browser contexts. The **flat session model** (`flatten: true`) is the recommended mode — all sessions share a single WebSocket connection and commands are routed via `sessionId`.

| Method / Event | Key fields | Use when |
|---|---|---|
| `Target.setAutoAttach` | `autoAttach: bool`, `waitForDebuggerOnStart: bool`, `flatten: bool` | Auto-attach to child targets (workers, service workers) |
| `Target.setDiscoverTargets` | `discover: bool` | Emit `targetCreated` for every known target |
| `Target.getTargets` | — | `targetInfos[]` — list all tabs, workers, frames |
| `Target.attachToTarget` | `targetId`, `flatten?: bool` | Manually attach to a target |
| `Target.detachFromTarget` | `sessionId` | Detach from a target |
| `Target.createBrowserContext` | `disposeOnDetach?: bool` | Incognito-style isolated context |
| `Target.disposeBrowserContext` | `browserContextId` | Destroy a browser context |
| Event: `attachedToTarget` | `{targetInfo, sessionId}` | Worker/tab attached — `sessionId` routes commands |
| Event: `targetCreated` | `{targetInfo}` | New target discovered |
| Event: `targetDestroyed` | `{targetId}` | Target closed |
| Event: `targetCrashed` | `{targetId, status, errorCode}` | Tab/worker crashed — restart it |

**`targetInfo.type` values:** `page` · `iframe` · `worker` (Web Worker) · `shared_worker` · `service_worker` · `browser` · `background_page`

**Routing a command to a worker session:**
```js
// Third argument to cdp.send() = sessionId → routes to that session
await cdp.send('Network.enable', {}, sessionId);
```

**Routing events from a worker session:**
```js
cdp.on('Network.webSocketCreated', (params, meta) => {
  if (meta?.sessionId === workerSessionId) { /* event from worker */ }
});
```

→ **Runnable implementation:** `SCRIPT_PATTERNS.md → WebSocket inside Workers`


## 15. Covered Domains — Quick Reference

| Domain | `enable` required? | Core purpose | Spec |
|---|---|---|---|
| Target | No | Tabs, workers, sessions, isolated contexts | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Target/) |
| Page | Yes | Navigate, screenshot, PDF, dialogs, script injection | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Page/) |
| Runtime | Yes (for events) | Execute JS, bindings, iframe contexts | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/) |
| DOM | Yes (for events) | Query/mutate DOM nodes | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DOM/) |
| Input | No | Mouse, keyboard, touch | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Input/) |
| Network | Yes | Monitor HTTP, cookies, headers, URL blocking | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Network/) |
| Fetch | Yes | Intercept & mock requests | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Fetch/) |
| Emulation | No | Viewport, UA, geo, timezone | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Emulation/) |
| Security | Yes | Ignore SSL errors, security state | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Security/) |
| Debugger | Yes | Breakpoints, step-through, call frame eval | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Debugger/) |
| CSS | Yes | Inspect/edit stylesheets | [↗](https://chromedevtools.github.io/devtools-protocol/tot/CSS/) |
| Storage | No | Clear storage per origin | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Storage/) |
| Performance | Yes | Collect perf metrics | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Performance/) |
| Log | Yes | Capture all console/browser output | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Log/) |
| Browser | No | Permissions, downloads, version | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Browser/) |
| ServiceWorker | Yes | SW lifecycle events, skip-waiting, unregister | [↗](https://chromedevtools.github.io/devtools-protocol/tot/ServiceWorker/) |


## 16. Other Domains — Official Spec Index

These domains exist in the full protocol. Consult the linked spec before using their methods, params, events, or experimental/deprecated features.

| Domain | Spec | Notes |
|---|---|---|
| Accessibility | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Accessibility/) | AX tree, ARIA roles, accessibility snapshots |
| Animation | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Animation/) | Pause/inspect/replay CSS & Web Animations |
| Audits | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Audits/) | Lighthouse-style issue detection |
| Autofill | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Autofill/) | Trigger browser autofill on forms |
| BackgroundService | [↗](https://chromedevtools.github.io/devtools-protocol/tot/BackgroundService/) | Monitor background fetch, sync, push |
| BluetoothEmulation | [↗](https://chromedevtools.github.io/devtools-protocol/tot/BluetoothEmulation/) | Simulate Bluetooth devices |
| CacheStorage | [↗](https://chromedevtools.github.io/devtools-protocol/tot/CacheStorage/) | Inspect/delete Cache API entries |
| Cast | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Cast/) | Control Chromecast sessions |
| Console | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Console/) | Legacy console API (prefer `Log` domain) |
| CrashReportContext | [↗](https://chromedevtools.github.io/devtools-protocol/tot/CrashReportContext/) | Attach metadata to crash reports |
| DeviceAccess | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DeviceAccess/) | Respond to WebUSB/WebBluetooth device prompts |
| DeviceOrientation | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DeviceOrientation/) | Override `DeviceOrientationEvent` values |
| DOMDebugger | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DOMDebugger/) | DOM mutation / event listener breakpoints |
| DOMSnapshot | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DOMSnapshot/) | Capture full DOM + layout snapshot in one call |
| DOMStorage | [↗](https://chromedevtools.github.io/devtools-protocol/tot/DOMStorage/) | Read/write localStorage & sessionStorage |
| EventBreakpoints | [↗](https://chromedevtools.github.io/devtools-protocol/tot/EventBreakpoints/) | Break on instrumentation events (e.g. `mousedown`) |
| Extensions | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Extensions/) | Load/manage Chrome extensions |
| FedCm | [↗](https://chromedevtools.github.io/devtools-protocol/tot/FedCm/) | Control Federated Credential Management dialogs |
| FileSystem | [↗](https://chromedevtools.github.io/devtools-protocol/tot/FileSystem/) | Inspect Origin Private File System (OPFS) |
| HeadlessExperimental | [↗](https://chromedevtools.github.io/devtools-protocol/tot/HeadlessExperimental/) | `beginFrame` control for headless rendering |
| HeapProfiler | [↗](https://chromedevtools.github.io/devtools-protocol/tot/HeapProfiler/) | Heap snapshots, allocation tracking |
| IndexedDB | [↗](https://chromedevtools.github.io/devtools-protocol/tot/IndexedDB/) | Read/clear IndexedDB databases |
| Inspector | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Inspector/) | `detached` event when DevTools is opened |
| IO | [↗](https://chromedevtools.github.io/devtools-protocol/tot/IO/) | Read stream handles returned by other domains (e.g. PDF) |
| LayerTree | [↗](https://chromedevtools.github.io/devtools-protocol/tot/LayerTree/) | Compositing layer tree, paint profiles |
| Media | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Media/) | Monitor `<video>`/`<audio>` player events |
| Memory | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Memory/) | Simulate memory pressure, DOM counter stats |
| Overlay | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Overlay/) | Highlight nodes, show layout grid overlays |
| PerformanceTimeline | [↗](https://chromedevtools.github.io/devtools-protocol/tot/PerformanceTimeline/) | Stream PerformanceObserver-style timeline events |
| Preload | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Preload/) | Monitor speculation rules / prerender status |
| Profiler | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Profiler/) | CPU profiler (start/stop, coverage) |
| PWA | [↗](https://chromedevtools.github.io/devtools-protocol/tot/PWA/) | Install/uninstall PWAs, change launch type |
| Schema | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Schema/) | List all supported domains (`Schema.getDomains`) |
| SmartCardEmulation | [↗](https://chromedevtools.github.io/devtools-protocol/tot/SmartCardEmulation/) | Simulate smart card readers |
| SystemInfo | [↗](https://chromedevtools.github.io/devtools-protocol/tot/SystemInfo/) | GPU info, display info, process list |
| Tethering | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Tethering/) | Port-forward between host and device (Android) |
| Tracing | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Tracing/) | Chrome trace events (performance timeline recording) |
| WebAudio | [↗](https://chromedevtools.github.io/devtools-protocol/tot/WebAudio/) | Inspect Web Audio graph nodes/edges |
| WebAuthn | [↗](https://chromedevtools.github.io/devtools-protocol/tot/WebAuthn/) | Virtual authenticator for passkey/FIDO testing |
| WebMCP | [↗](https://chromedevtools.github.io/devtools-protocol/tot/WebMCP/) | MCP server discovery in the browser |


*Full reference: https://chromedevtools.github.io/devtools-protocol/tot/*  
*TypeScript types (optional, for IDE autocomplete): `npm install devtools-protocol`*  
*Runtime: Node.js 22+ — native WebSocket built-in, no `ws` package needed*
