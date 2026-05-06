# Chrome DevTools Protocol (CDP) — Agent Reference

> Use `cdp-runner.mjs` / `cdp-sandbox.mjs` — connection is handled automatically. This reference covers domain APIs, method params, events, and error codes only.


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

### Click an element
```ts
const { root } = await tab.send<{ root: { nodeId: number } }>('DOM.getDocument', { depth: 0 });
const { nodeId } = await tab.send<{ nodeId: number }>('DOM.querySelector', { nodeId: root.nodeId, selector: '#submit' });
await tab.send('DOM.scrollIntoViewIfNeeded', { nodeId });
const { model } = await tab.send<{ model: { content: number[] } }>('DOM.getBoxModel', { nodeId });
const [x1,,x3,,,,,y1,,,,y3] = model.content; // 8-point polygon
const cx = (x1 + x3) / 2, cy = (y1 + y3) / 2;
await tab.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: cx, y: cy, button: 'left', clickCount: 1 });
await tab.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: cx, y: cy, button: 'left', clickCount: 1 });
```

### Fill a text field
```ts
const { root } = await tab.send<{ root: { nodeId: number } }>('DOM.getDocument', { depth: 0 });
const { nodeId } = await tab.send<{ nodeId: number }>('DOM.querySelector', { nodeId: root.nodeId, selector: 'input[name="email"]' });
await tab.send('DOM.focus', { nodeId });
await tab.send('Input.insertText', { text: 'user@example.com' });
```

### Wait for navigation
```ts
const { frameId } = await tab.send<{ frameId: string }>('Page.navigate', { url: 'https://example.com' });
await new Promise<void>(resolve =>
  browser.on('Page.frameStoppedLoading', ({ frameId: fid }) => { if (fid === frameId) resolve(); })
);
```

### Wait for an element (no native waitForSelector — must poll)

**Basic existence poll** — resolves as soon as `DOM.querySelector` returns a non-zero `nodeId`. Does NOT check visibility, enabled state, or pointer-events.
```ts
type TabSend = <T>(method: string, params?: Record<string, unknown>) => Promise<T>;

async function waitForSelector(send: TabSend, selector: string, timeoutMs = 10000): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  const { root } = await send<{ root: { nodeId: number } }>('DOM.getDocument', { depth: 0 });
  while (Date.now() < deadline) {
    const { nodeId } = await send<{ nodeId: number }>('DOM.querySelector', { nodeId: root.nodeId, selector });
    if (nodeId !== 0) return nodeId;
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error(`Timeout waiting for: ${selector}`);
}
```

Need full actionability check (visible + enabled + pointer)? → `SCRIPT_PATTERNS.md → waitForSelector with Actionability`

### Run JS in a specific iframe
```ts
// Capture contextId per frame from Runtime.executionContextCreated
const iframeContexts = new Map<string, number>(); // frameId → contextId
browser.on('Runtime.executionContextCreated', ({ context }: any) => {
  if (context.auxData?.frameId) iframeContexts.set(context.auxData.frameId, context.id);
});
await tab.send('Runtime.enable');

// Then target that iframe's context
const { result } = await tab.send<{ result: { value: unknown } }>('Runtime.evaluate', {
  expression: 'document.querySelector("input").value',
  contextId: iframeContexts.get(iframeFrameId),
  returnByValue: true,
});
```

### Let page JS call back into the agent
```ts
await tab.send('Runtime.addBinding', { name: 'agentCallback' });
// In page JS:  window.agentCallback(JSON.stringify({ event: 'done', data: 42 }))
browser.on('Runtime.bindingCalled', ({ name, payload }: any) => {
  if (name === 'agentCallback') {
    const data = JSON.parse(payload as string);
    console.log('Page called back:', data);
  }
});
```

### Inject script before page JS runs (anti-bot, stubs)
```ts
const { identifier } = await tab.send<{ identifier: string }>(
  'Page.addScriptToEvaluateOnNewDocument',
  { source: `Object.defineProperty(navigator, 'webdriver', { get: () => false });` }
);
// later:
await tab.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
```

### Mock an API call
```ts
await tab.send('Fetch.enable', { patterns: [{ urlPattern: '*/api/data*', requestStage: 'Request' }] });

browser.on('Fetch.requestPaused', async ({ requestId }) => {
  const body = Buffer.from(JSON.stringify({ mocked: true })).toString('base64');
  await tab.send('Fetch.fulfillRequest', {
    requestId,
    responseCode: 200,
    responseHeaders: [{ name: 'Content-Type', value: 'application/json' }],
    body,
  });
});
```

### File upload

**Method:** `DOM.setFileInputFiles` — sets absolute file paths on `input[type="file"]`.  
**Gotchas:** files must be absolute paths; always dispatch `input` + `change` events after (frameworks won't react otherwise); if nodeId is 0, page is still loading — use `waitForSelector` first.

→ **Runnable implementation:** `SCRIPT_PATTERNS.md → File Upload`

### Wait for network idle

**Concept:** Track every `Network.requestWillBeSent` / `loadingFinished` pair — resolve when pending count hits 0 and stays 0 for `idleMs`. More reliable than `setTimeout`.  
**Key rule:** attach listener **before** `Page.navigate` — events fire the moment navigation starts.  
**Signature:** `waitForNetworkIdle(cdp, { idleMs?: number, timeoutMs?: number }): Promise<void>`

→ **Runnable implementation:** `SCRIPT_PATTERNS.md → waitForNetworkIdle`

### Wait for selector with actionability

**Concept:** No native `waitForSelector` in CDP — must poll via `Runtime.evaluate`. Checks visibility (`getBoundingClientRect`, `display`, `visibility`, `opacity`), enabled state (`!el.disabled`), and pointer accessibility (`pointerEvents !== 'none'`). Returns `nodeId` for DOM operations.  
**Signature:** `waitForSelector(cdp, selector, { timeoutMs?, checkVisible?, checkEnabled?, checkPointer?, pollMs? }): Promise<nodeId>`

→ **Runnable implementation:** `SCRIPT_PATTERNS.md → waitForSelector with Actionability`

### Query elements inside shadow DOM

**Key facts:**
- `DOM.querySelector` / `DOM.querySelectorAll` **never** cross shadow boundaries
- Use `DOM.getDocument({ pierce: true })` to get shadow roots in the tree (appear as nodeType 11)
- Use `Runtime.evaluate` with a recursive `pierce()` helper to search across all shadow roots
- Closed shadow roots (`mode: 'closed'`) are inaccessible — no CDP bypass exists

→ **Runnable implementation (3 patterns):** `SCRIPT_PATTERNS.md → Shadow DOM — Querying Inside Shadow Roots`

### Isolated task context (no cookie/storage leakage)
```ts
const { browserContextId } = await browser.send<{ browserContextId: string }>(
  'Target.createBrowserContext', { disposeOnDetach: true }
);
const tab = await openTab(browser, 'about:blank'); // pass browserContextId inside openTab if needed
// ... do work ...
await browser.send('Target.closeTarget', { targetId: tab.targetId });
await browser.send('Target.disposeBrowserContext', { browserContextId });
```

### Ignore SSL errors
```ts
await tab.send('Security.enable');
await tab.send('Security.setIgnoreCertificateErrors', { ignore: true });
await tab.send('Page.navigate', { url: 'https://self-signed.local' });
```

### Error signals — always handle these
- `Target.targetCrashed` → restart the target
- `Runtime.exceptionThrown` → unhandled JS error on the page
- `Log.entryAdded` with `level: "error"` → browser/network error
- `Fetch.requestPaused` with no handler → page hangs forever (deadlock)
- `Page.navigate` with `errorText` in response → navigation failed


## 13. Covered Domains — Quick Reference

| Domain | `enable` required? | Core purpose | Spec |
|---|---|---|---|
| Target | No | Tabs, sessions, isolated contexts | [↗](https://chromedevtools.github.io/devtools-protocol/tot/Target/) |
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


## 14. Other Domains — Not covered in this doc

These domains exist in the full protocol. Consult the linked spec when needed.

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
| ServiceWorker | [↗](https://chromedevtools.github.io/devtools-protocol/tot/ServiceWorker/) | Inspect/update/skip-waiting service workers |
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
