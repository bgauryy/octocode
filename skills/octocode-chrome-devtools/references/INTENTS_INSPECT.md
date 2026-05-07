# CDP Inspection Surface Intent Details

## security

**Trigger phrases:** "security", "cookies", "tokens", "auth", "CSP", "headers", "data exfil", "what's being leaked", "credentials", "session", "localStorage", "is this safe"

**Domains:** `Network.enable`, `Runtime.enable`, `DOM.enable`, `Page.enable`

**Key events/methods:**
- `Network.getCookies({ urls: [TARGET_URL] })` → cookies scoped to the target URL only (use this — `getAllCookies` returns the entire browser jar and floods output with third-party ad cookies)
- `Network.requestWillBeSent` + `Network.getRequestPostData(requestId)` → actual POST bodies
- `Network.responseReceived` → inspect response headers: `Content-Security-Policy`, `Strict-Transport-Security`, `X-Frame-Options`, `X-Content-Type-Options`
- `Runtime.evaluate` → localStorage keys and sizes only; never emit stored values
- `Runtime.evaluate` → sessionStorage keys and sizes only; never emit stored values
- `DOMDebugger.getEventListeners({objectId})` → listeners on `document`, `window`, `input[type=password]` (get objectId via `Runtime.evaluate({expression:"document"})`)
- `Runtime.evaluate` → `Object.keys(Object.getPrototypeOf(Object.prototype))` → prototype pollution check

**Output prefixes:** `[SECURITY]` `[NETWORK]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- Cookie missing `httpOnly` → `[FINDING] COOKIE_NO_HTTPONLY: ${name}`
- Cookie missing `secure` flag → `[FINDING] COOKIE_NO_SECURE: ${name}`
- Cookie `sameSite` is `None` without `secure` → `[FINDING] COOKIE_SAMESITE_NONE_INSECURE: ${name}`
- POST body contains `token`, `password`, `secret`, `apiKey`, `jwt` → `[FINDING] SENSITIVE_IN_POST: ${url}`
- Response missing `Content-Security-Policy` → `[FINDING] MISSING_CSP: ${url}`
- CSP contains `unsafe-eval` or `unsafe-inline` → `[FINDING] WEAK_CSP: ${directive} in ${url}`
- Response missing `Strict-Transport-Security` → `[FINDING] MISSING_HSTS: ${url}`
- Response missing `X-Frame-Options` → `[FINDING] MISSING_XFRAME: ${url}`
- `localStorage` key matches `token|auth|jwt|secret|key|password` → `[FINDING] SENSITIVE_IN_STORAGE: ${key}`
- `keydown`/`keyup` listener on `document` from third-party script URL → `[FINDING] POSSIBLE_KEYLOGGER: ${listenerUrl}`
- `copy`/`paste` listener on `document` → `[FINDING] CLIPBOARD_LISTENER: possible clipboard hijack`
- `Object.prototype` has unexpected own properties → `[FINDING] PROTOTYPE_POLLUTION: ${keys}`
- Request to unknown external domain → `[FINDING] DATA_EXFIL_SUSPECT: ${url}`

**Pattern:** See **Security Audit** in `SCRIPT_PATTERNS.md` and trim it to the specific security question.

## websocket

**Trigger phrases:** "websocket", "WS", "socket", "real-time", "socket frames", "what's sent over WS"

**Domains:** `Network.enable`

**Key events/methods:**
- `Network.webSocketCreated` → `{requestId, url, initiator}` — WS object constructed (`new WebSocket(url)`) — **this is the correct creation event, not `requestWillBeSent`**
- `Network.webSocketHandshakeResponseReceived` → headers, status
- `Network.webSocketFrameSent` → `{requestId, timestamp, response: {payloadData, opcode}}` — opcode 1 = text, 2 = binary
- `Network.webSocketFrameReceived` → `{requestId, timestamp, response: {payloadData, opcode}}`
- `Network.webSocketClosed` → `{requestId, timestamp}`

**Output prefixes:** `[NETWORK]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- WS endpoint on unknown third-party domain → `[FINDING] WS_UNKNOWN_HOST: ${url}`
- Frame payload contains `token`, `password`, `key` → `[FINDING] SENSITIVE_IN_WS_FRAME: ${url}` (value redacted)
- Base64-encoded payload (matches `/^[A-Za-z0-9+/]{40,}={0,2}$/`) → `[FINDING] WS_BASE64_FRAME: possible encoded data`
- Frame size > 100KB → `[FINDING] LARGE_WS_FRAME: ${kb}KB sent` *(threshold is a default — adjust for your protocol)*
- opcode === 2 → binary frame; base64-decode payload for inspection

**WS inside Web Workers:** `Network.webSocketCreated` does **not** fire on the main session for WebSockets opened inside a Web Worker or Shared Worker. To capture those, enable the `workers` intent first (`Target.setAutoAttach`), then call `cdp.send('Network.enable', {}, sessionId)` on each attached worker session. See the **workers** intent and **WebSocket inside Workers** pattern in `SCRIPT_PATTERNS.md`.

## service-worker

**Trigger phrases:** "service worker", "SW lifecycle", "PWA worker", "offline cache", "push notifications", "background sync", "SW registered", "what service workers are running", "is there a service worker"

**Domains:** `ServiceWorker.enable` + `Target.setAutoAttach` (to receive the SW as an attachable target)

**Key events/methods:**
- `ServiceWorker.enable` — must call before navigation to receive SW events
- Event: `ServiceWorker.workerRegistrationUpdated` → `{registrations[]}` — fires when a SW registration is created or deleted; each `registration` has `{registrationId, scopeURL, isDeleted}`
- Event: `ServiceWorker.workerVersionUpdated` → `{versions[]}` — fires on **every state transition**; each `version` has `{registrationId, versionId, scriptURL, status, runningStatus}`:
  - `status`: `"new"` → `"installing"` → `"installed"` → `"activating"` → `"activated"` → `"redundant"`
  - `runningStatus`: `"stopped"` → `"starting"` → `"running"` → `"stopping"`
- `ServiceWorker.disable` — call at cleanup to stop receiving events

**SW methods (imperative):**
- `ServiceWorker.skipWaiting({scopeURL})` — force a waiting SW to activate immediately
- `ServiceWorker.updateRegistration({scopeURL})` — trigger a SW update check
- `ServiceWorker.unregister({scopeURL})` — unregister a SW

**Output prefixes:** `[SW]` `[FINDING]` `[MONITOR]`

**`[FINDING]` conditions to emit:**
- New registration for a non-extension scope → `[FINDING] SW_REGISTERED: ${scopeURL}`
- SW reaches `activated` state → `[FINDING] SW_ACTIVATED: ${scriptURL}`
- SW `isDeleted` = true (unregistered or evicted) → `[FINDING] SW_REMOVED: ${scopeURL}`
- SW script is on a third-party domain → `[FINDING] SW_THIRD_PARTY_SCRIPT: ${scriptURL}`

**Tip — navigator API for a point-in-time snapshot (after page load):**
```js
const regs = await cdp.send('Runtime.evaluate', {
  expression: `navigator.serviceWorker.getRegistrations().then(r=>JSON.stringify(r.map(x=>({scope:x.scope,state:(x.active||x.installing||x.waiting)?.state,script:(x.active||x.installing||x.waiting)?.scriptURL}))))`,
  awaitPromise: true, returnByValue: false,
});
const sws = JSON.parse(regs.result.value ?? '[]');
```
Use the CDP domain (`ServiceWorker.enable` + events) for **live lifecycle tracking**; use `navigator.serviceWorker.getRegistrations()` for a **snapshot** after load.

**Combine with:** `workers` (to attach to the SW target and capture its network traffic), `storage` (to read CacheStorage entries managed by the SW).

## workers

**Trigger phrases:** "web worker", "shared worker", "worker thread", "background worker", "blob worker", "WS inside worker", "what workers are running", "worker targets", "worker network traffic"

**Domains:** `Target.setAutoAttach` + `Target.setDiscoverTargets` — must be called **before navigation**

**Key events/methods:**
- `Target.setAutoAttach({autoAttach: true, waitForDebuggerOnStart: false, flatten: true})` — auto-attach to all child targets (workers, service workers) with flat session model
- `Target.setDiscoverTargets({discover: true})` — emit `Target.targetCreated` for worker targets that Chrome discovers
- Event: `Target.attachedToTarget` → `{targetInfo, sessionId}` — fires when a worker is attached; `targetInfo.type` is one of `"worker"` (Web Worker), `"shared_worker"`, `"service_worker"`
- Event: `Target.targetCreated` → `{targetInfo}` — fires when a worker target is created (type = worker/shared_worker/service_worker)
- `Target.getTargets({})` → `{targetInfos[]}` — list all known targets including workers (use after page settles)
- `Target.detachFromTarget({sessionId})` — detach from a worker when done

**Enabling Network on a worker (flat session model):**

When `flatten: true` is set in `setAutoAttach`, worker sessions share the same WebSocket connection. Pass `sessionId` as the **third argument** to `cdp.send()` to route commands to that session:

```js
cdp.on('Target.attachedToTarget', async ({ targetInfo, sessionId }) => {
  if (!['worker', 'shared_worker', 'service_worker'].includes(targetInfo.type)) return;
  console.log(`[WORKER] Attached: type=${targetInfo.type} url=${targetInfo.url || '(blob)'}`);
  // Enable Network on this worker's session to capture its HTTP + WS traffic
  await cdp.send('Network.enable', {}, sessionId);
  // All Network events from this worker now carry { sessionId } in their metadata.
  // Route them with a wrapper that filters by sessionId.
});
```

**Routing worker events (filter by sessionId):**
```js
// Generic: listen to an event on a specific worker session
function onWorkerEvent(event, workerSessionId, handler) {
  cdp.on(event, (params, meta) => {
    if (meta?.sessionId === workerSessionId) handler(params);
  });
}
// Then:
onWorkerEvent('Network.webSocketCreated', sessionId, ({ requestId, url }) => {
  console.log(`[WORKER] WS inside worker: ${url}`);
});
```

**Output prefixes:** `[WORKER]` `[FINDING]` `[MONITOR]`

**`[FINDING]` conditions to emit:**
- Worker URL is a blob: URL → `[FINDING] WORKER_BLOB: ${url}` (expected but opaque; blob workers cannot be inspected via source)
- Shared Worker detected → `[FINDING] SHARED_WORKER: ${url}` (multi-tab communication channel)
- Service Worker detected → `[FINDING] SERVICE_WORKER_TARGET: ${url}` (complements service-worker intent)
- WS connection inside a worker → `[FINDING] WS_IN_WORKER: type=${type} url=${wsUrl}`

**Cleanup:**
```js
await cdp.send('Target.setAutoAttach', { autoAttach: false, waitForDebuggerOnStart: false, flatten: true });
await cdp.send('Target.setDiscoverTargets', { discover: false });
```

**Combine with:** `websocket` (capture WS frames inside workers), `service-worker` (track SW lifecycle alongside its target session), `network` (capture HTTP requests made by workers).

## intercept

**Trigger phrases:** "intercept", "mock", "block request", "fake response", "modify request", "inject headers", "replace API response"

**Domains:** `Fetch.enable` (call before navigation, no explicit `enable` method — call `Fetch.enable` with `patterns`)

**Key events/methods:**
- `Fetch.enable({patterns:[{urlPattern:"*", requestStage:"Request"}]})` → intercept all requests
- Event: `Fetch.requestPaused` → `{requestId, request, responseStatusCode, responseHeaders}`
- `Fetch.continueRequest({requestId})` → pass through unchanged
- `Fetch.fulfillRequest({requestId, responseCode:200, body: btoa(JSON.stringify(mockData))})` → return mock
- `Fetch.continueRequest({requestId, headers:[...]})` → modify headers before continuing

**Output prefixes:** `[NETWORK]` `[FINDING]`

**Note:** `Fetch.enable` must be called **before** navigation. `body` in `fulfillRequest` must be base64-encoded.

## screenshot

**Trigger phrases:** "screenshot", "capture", "take a photo", "visual", "PDF", "print page"

**Domains:** `Page.enable`

**Key events/methods:**
- `Page.captureScreenshot({format:"png"})` → `data` (base64) → write to `cdp.outputDir/screenshot-<slug>.png`
- `Page.printToPDF({printBackground:true})` → `data` (base64) → write to `cdp.outputDir/<slug>.pdf`
- Wait for `Page.loadEventFired` before capturing — capturing too early yields a blank or partial screenshot

**Output prefixes:** `[SCREENSHOT]`

**Write file pattern — use `cdp.outputDir` (sandbox-safe, cross-platform):**
```js
const { writeFileSync } = await import('fs');
const { join } = await import('path');
const screenshotPath = join(cdp.outputDir, 'screenshot.png');
writeFileSync(screenshotPath, Buffer.from(data, 'base64'));
console.log(`[SCREENSHOT] ${screenshotPath}`);
```

## accessibility

**Trigger phrases:** "accessibility", "a11y", "aria", "screen reader", "wcag", "alt text", "labels"

**Domains:** `DOM.enable`, `Runtime.enable`, `Accessibility.enable`

**Key events/methods:**
- `Accessibility.getFullAXTree` → full semantic tree with `role`, `name`, `description`, `states`
- `Runtime.evaluate` → `img:not([alt])`, `input:not([aria-label])`, `button:empty`, `[role]` checks

**Output prefixes:** `[DOM]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- AX node with `role:"img"` and empty `name` → `[FINDING] AX_MISSING_ALT: image has no accessible name`
- AX node with `role:"button"` and empty `name` → `[FINDING] AX_EMPTY_BUTTON`
- Input without label in AX tree → `[FINDING] AX_UNLABELED_INPUT`
- Page has no `role:"main"` landmark → `[FINDING] AX_NO_MAIN_LANDMARK`
- Heading order skips levels (h1 → h3) → `[FINDING] AX_HEADING_SKIP`

## supply-chain

**Trigger phrases:** "third-party scripts", "external scripts", "CDN", "supply chain", "what scripts load", "external JS"

**Domains:** `Network.enable`, `Runtime.enable`, `Page.enable`

**Key events/methods:**
- `Network.requestWillBeSent` → filter for `.js` requests where origin ≠ page hostname
- `Network.responseReceived` → check `Subresource-Integrity` / `integrity` attribute presence
- `Runtime.evaluate` → `[...document.querySelectorAll('script[src]')].map(s=>({src:s.src,integrity:s.integrity||null}))` → list all script tags

**Output prefixes:** `[NETWORK]` `[SECURITY]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- Third-party JS loaded without SRI hash → `[FINDING] NO_SRI: ${url}`
- Script from unknown CDN (not from well-known CDNs) → `[FINDING] UNKNOWN_CDN: ${url}`
- More than 10 distinct third-party domains → `[FINDING] HIGH_THIRD_PARTY_COUNT: ${n} external domains` *(adjust for your expected vendor count)*
- Script loaded over HTTP (not HTTPS) → `[FINDING] INSECURE_SCRIPT_LOAD: ${url}`
- `window` gains new property after third-party script loads → `[FINDING] WINDOW_POLLUTION: ${key} added by ${url}`

## full-audit

**Trigger phrases:** "full audit", "everything", "all checks", "complete audit", "check it all"

**Domains:** Enable all: `Network.enable`, `Runtime.enable`, `Log.enable`, `Performance.enable`, `DOM.enable`, `CSS.enable`, `HeapProfiler.enable`, `Page.enable`

**Strategy:** Combine all intents above into one script. Run in this order:
1. Enable all domains
2. Attach all event listeners (network, console, exceptions, WS)
3. Navigate to target
4. Wait for `Page.loadEventFired`
5. Run synchronous checks: `Performance.getMetrics`, DOM queries, `Network.getCookies({ urls: [TARGET_URL] })` (not `getAllCookies` — that returns the entire browser jar), localStorage key/size inventory, `DOMDebugger.getEventListeners`
6. Wait 5–10s for async activity
7. Emit `[METRIC]` summary for each category

**Output prefixes:** All of the above.

**Emit a summary block at the end:**
```
[METRIC] === AUDIT SUMMARY ===
[METRIC] Network requests: N  Errors: N
[METRIC] Console errors: N  Exceptions: N
[METRIC] JS heap: NMB  DOM nodes: N
[METRIC] Cookies: N  Missing httpOnly: N
[METRIC] Missing CSP: yes/no  Missing HSTS: yes/no
[METRIC] Third-party scripts: N  Without SRI: N
```
