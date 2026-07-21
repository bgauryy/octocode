# CDP Async And Worker Pattern Details

## waitForNetworkIdle

Event-driven wait until all in-flight network requests finish. More reliable than `setTimeout` because it tracks every `requestWillBeSent` / `loadingFinished` pair, including late XHR/fetch calls fired by page JS after DOM load.

```js
// Requires: Network.enable (already active)
// Attach listeners BEFORE navigating — events fire immediately on navigation start

async function waitForNetworkIdle(cdp, { idleMs = 500, timeoutMs = 30000 } = {}) {
  return new Promise((resolve, reject) => {
    let pending = 0;
    let idleTimer = null;

    const deadline = setTimeout(() => {
      clearTimeout(idleTimer);
      reject(new Error(`waitForNetworkIdle timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    const scheduleIdle = () => {
      clearTimeout(idleTimer);
      if (pending === 0) {
        idleTimer = setTimeout(() => { clearTimeout(deadline); resolve(); }, idleMs);
      }
    };

    cdp.on('Network.requestWillBeSent',    () => { pending++; clearTimeout(idleTimer); });
    cdp.on('Network.loadingFinished',      () => { pending = Math.max(0, pending - 1); scheduleIdle(); });
    cdp.on('Network.loadingFailed',        () => { pending = Math.max(0, pending - 1); scheduleIdle(); });
    cdp.on('Network.requestServedFromCache', () => { pending = Math.max(0, pending - 1); scheduleIdle(); });

    scheduleIdle(); // resolve immediately if nothing is already in-flight
  });
}

export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Page.enable', {});

  // Attach idle listener BEFORE navigating
  const idlePromise = waitForNetworkIdle(cdp, { idleMs: 500, timeoutMs: 15000 });

  await cdp.send('Page.navigate', { url: 'https://example.com' });
  await idlePromise;

  console.log('[METRIC] Network idle — all requests finished');
}
```

**Alternative: `Page.lifecycleEvent` (simpler, less precise)**

```js
await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true });
await new Promise(resolve =>
  cdp.on('Page.lifecycleEvent', ({ name }) => {
    if (name === 'networkIdle') resolve();
  })
);
```

**Parameters:**
- `idleMs` (default 500ms): quiet window — how long zero pending requests must hold before resolving
- `timeoutMs` (default 30000ms): hard ceiling — rejects if network never goes idle

**`requestServedFromCache` note:** cache hits never fire `loadingFinished`, so they must be counted as completions separately.

## waitForSelector with Actionability

Wait for an element to be present **and** actionable — visible, enabled, and reachable by pointer. Prevents premature clicks on elements that exist in the DOM but are animating, hidden behind overlays, or inside disabled fieldsets.

```js
// Requires: Runtime.enable, DOM.enable
// Checks: in DOM + non-zero size + not display:none/visibility:hidden/opacity:0 + not disabled + pointer-events != none

async function waitForSelector(cdp, selector, {
  timeoutMs    = 10000,
  checkVisible = true,  // non-zero bounding box + not hidden via CSS
  checkEnabled = true,  // not disabled (self or ancestor)
  checkPointer = true,  // CSS pointer-events !== 'none'
  pollMs       = 150,
} = {}) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    const { result } = await cdp.send('Runtime.evaluate', {
      expression: `(function() {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) return null;
        const s   = window.getComputedStyle(el);
        const r   = el.getBoundingClientRect();
        return {
          visible: r.width > 0 && r.height > 0
                   && s.display     !== 'none'
                   && s.visibility  !== 'hidden'
                   && s.opacity     !== '0',
          enabled: !el.disabled && !el.closest('[disabled]'),
          pointer: s.pointerEvents !== 'none',
        };
      })()`,
      returnByValue: true,
    });

    const state = result.value;
    if (!state) { await new Promise(r => setTimeout(r, pollMs)); continue; }

    const ready = (!checkVisible || state.visible)
               && (!checkEnabled || state.enabled)
               && (!checkPointer || state.pointer);

    if (ready) {
      const { root }   = await cdp.send('DOM.getDocument', { depth: 0 });
      const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector });
      console.log(`[AUTOMATE] "${selector}" ready — visible:${state.visible} enabled:${state.enabled} pointer:${state.pointer}`);
      return nodeId; // use with DOM.getBoxModel + Input.dispatchMouseEvent to click
    }

    await new Promise(r => setTimeout(r, pollMs));
  }

  throw new Error(`waitForSelector("${selector}") timed out after ${timeoutMs}ms`);
}

// Usage: click a button only once it is visible and enabled
export async function run(cdp) {
  await cdp.send('DOM.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Page.navigate', { url: 'https://example.com' });

  const nodeId = await waitForSelector(cdp, '#submit-btn');

  // Real mouse click via CDP Input (required for some frameworks)
  await cdp.send('DOM.scrollIntoViewIfNeeded', { nodeId });
  const { model } = await cdp.send('DOM.getBoxModel', { nodeId });
  const [x1,,x3,,,,,y1,,,,y3] = model.content; // 8-point polygon
  const cx = (x1 + x3) / 2, cy = (y1 + y3) / 2;
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: cx, y: cy, button: 'left', clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: cx, y: cy, button: 'left', clickCount: 1 });
  console.log('[AUTOMATE] clicked #submit-btn');
}
```

**Actionability checks explained:**
- **visible**: `getBoundingClientRect().width/height > 0` AND not `display:none` / `visibility:hidden` / `opacity:0`
- **enabled**: `!el.disabled` AND not inside a `[disabled]` fieldset/ancestor
- **pointer**: `pointer-events !== 'none'` — catches elements blocked by CSS overlays

**Skip flags:**
- `checkVisible: false` — for off-screen or hidden inputs you set by value, not click
- `checkEnabled: false` — for read-only fields
- `checkPointer: false` — for elements that use JS click handlers bypassing CSS pointer-events

## Service Worker Lifecycle

Tracks every Service Worker state transition via the `ServiceWorker` CDP domain. Captures registration, full lifecycle (new → installing → installed → activating → activated → redundant), and the navigator API snapshot.

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true });
  await cdp.send('ServiceWorker.enable', {}); // must be before navigation

  const swVersions = new Map(); // scriptURL → latest status/runningStatus key
  const swRegistrations = new Set(); // scopeURLs seen

  cdp.on('ServiceWorker.workerRegistrationUpdated', ({ registrations }) => {
    for (const r of registrations) {
      if (r.scopeURL.startsWith('chrome-extension:')) continue;
      const label = r.isDeleted ? 'DELETED' : 'REGISTERED';
      console.log(`[SW] ${label}: scope=${r.scopeURL} id=${r.registrationId}`);
      if (!r.isDeleted) {
        swRegistrations.add(r.scopeURL);
        console.log(`[FINDING] SW_REGISTERED: ${r.scopeURL}`);
      } else {
        console.log(`[FINDING] SW_REMOVED: ${r.scopeURL}`);
      }
    }
  });

  cdp.on('ServiceWorker.workerVersionUpdated', ({ versions }) => {
    for (const v of versions) {
      if (!v.scriptURL || v.scriptURL.startsWith('chrome-extension:')) continue;
      const key = `${v.status}/${v.runningStatus}`;
      const prev = swVersions.get(v.scriptURL);
      if (prev === key) continue; // deduplicate repeated events
      swVersions.set(v.scriptURL, key);
      const fn = v.scriptURL.split('/').slice(-1)[0].split('?')[0];
      console.log(`[SW] VERSION: ${fn} → ${v.status}/${v.runningStatus} (script=${v.scriptURL})`);
      if (v.status === 'activated') console.log(`[FINDING] SW_ACTIVATED: ${v.scriptURL}`);
      if (v.status === 'redundant')  console.log(`[FINDING] SW_REDUNDANT: ${v.scriptURL}`);
      const swHost = (() => { try { return new URL(v.scriptURL).hostname; } catch { return ''; } })();
      const pageHost = (() => { try { return new URL(TARGET_URL).hostname; } catch { return ''; } })();
      if (swHost && pageHost && swHost !== pageHost) console.log(`[FINDING] SW_THIRD_PARTY_SCRIPT: ${v.scriptURL}`);
    }
  });

  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise(r => setTimeout(r, 15000)); // allow SW to install + activate

  // Point-in-time snapshot via navigator API
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `(async () => {
      if (!navigator.serviceWorker) return '[]';
      const regs = await navigator.serviceWorker.getRegistrations();
      return JSON.stringify(regs.map(r => ({
        scope: r.scope,
        updateViaCache: r.updateViaCache,
        state: (r.active || r.installing || r.waiting)?.state ?? 'unknown',
        scriptURL: (r.active || r.installing || r.waiting)?.scriptURL ?? null,
      })));
    })()`,
    awaitPromise: true, returnByValue: false,
  });
  const sws = JSON.parse(result.value ?? '[]');

  console.log(`[METRIC] SW registrations (live CDP): ${swRegistrations.size}`);
  console.log(`[METRIC] SW registrations (navigator API): ${sws.length}`);
  for (const s of sws) {
    console.log(`[SW] SNAPSHOT: scope=${s.scope} state=${s.state} script=${s.scriptURL}`);
  }

  await cdp.send('ServiceWorker.disable', {});
}
```

**Key facts:**
- `ServiceWorker.enable` must be called before navigation — events are not replayed retroactively
- `workerVersionUpdated` fires repeatedly for the same version as state advances — deduplicate on `scriptURL + status/runningStatus`
- `workerRegistrationUpdated` fires for ALL known registrations (including from prior sessions, extensions) on enable — filter `chrome-extension:` scopes
- `status` lifecycle order: `new` → `installing` → `installed` → `activating` → `activated` → `redundant`
- `runningStatus` lifecycle: `stopped` → `starting` → `running` → `stopping`
- Use `ServiceWorker.skipWaiting({scopeURL})` to force a waiting SW to activate immediately (useful in tests)

## WebSocket inside Workers

Web Workers and Shared Workers can open their own WebSocket connections. These do **not** emit `Network.webSocketCreated` on the main session — you must attach to each worker via `Target.setAutoAttach` and enable `Network` on its session.

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true });
  await cdp.send('ServiceWorker.enable', {});
  // flatten:true = worker sessions share the same CDP WebSocket; sessionId routes commands
  await cdp.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await cdp.send('Target.setDiscoverTargets', { discover: true });

  const wsMap = new Map(); // `${sessionLabel}::${requestId}` → { url, frames }

  // Helper: attach Network WS listeners to any session (main page OR worker)
  function listenWS(session, label) {
    function on(event, handler) {
      cdp.on(event, (params, meta) => {
        // main page: no meta.sessionId; worker: meta.sessionId matches
        const isTarget = label === 'MAIN'
          ? !meta?.sessionId
          : meta?.sessionId === session.sessionId;
        if (isTarget) handler(params);
      });
    }

    on('Network.webSocketCreated', ({ requestId, url }) => {
      wsMap.set(`${label}::${requestId}`, { url, sent: [], recv: [] });
      console.log(`[WORKER] WS OPENED [${label}]: ${url}`);
      const wsHost = (() => { try { return new URL(url).hostname; } catch { return '?'; } })();
      console.log(`[FINDING] WS_IN_WORKER: type=${label} host=${wsHost} url=${url}`);
    });

    on('Network.webSocketFrameSent', ({ requestId, response }) => {
      const ws = wsMap.get(`${label}::${requestId}`);
      if (!ws) return;
      const p = response?.payloadData ?? '';
      ws.sent.push({ bytes: p.length, opcode: response?.opcode });
      console.log(`[WORKER] WS SENT [${label}] ${p.length}B op=${response?.opcode}`);
      if (/token|password|secret|key|auth|jwt/i.test(p))
        console.log(`[FINDING] SENSITIVE_IN_WS_SENT [${label}]: value redacted`);
    });

    on('Network.webSocketFrameReceived', ({ requestId, response }) => {
      const ws = wsMap.get(`${label}::${requestId}`);
      if (!ws) return;
      const p = response?.payloadData ?? '';
      ws.recv.push({ bytes: p.length, opcode: response?.opcode });
      console.log(`[WORKER] WS RECV [${label}] ${p.length}B op=${response?.opcode}`);
    });

    on('Network.webSocketClosed', ({ requestId }) => {
      const ws = wsMap.get(`${label}::${requestId}`);
      if (!ws) return;
      console.log(`[WORKER] WS CLOSED [${label}]: ${ws.url} S:${ws.sent.length} R:${ws.recv.length}`);
    });
  }

  // Listen on the main page session
  listenWS({ sessionId: null }, 'MAIN');

  // When a worker attaches, enable Network on it and listen there too
  let workerCount = 0;
  cdp.on('Target.attachedToTarget', async ({ targetInfo, sessionId }) => {
    const t = targetInfo.type;
    if (!['worker', 'shared_worker', 'service_worker'].includes(t)) return;
    workerCount++;
    const label = `${t.toUpperCase()}(${workerCount})`;
    console.log(`[WORKER] Attached: type=${t} label=${label} url=${targetInfo.url || '(blob)'}`);
    console.log(`[FINDING] WORKER_${t.toUpperCase()}: ${targetInfo.url || '(blob)'}`);
    try {
      await cdp.send('Network.enable', {}, sessionId); // route to worker session
      listenWS({ sessionId }, label);
    } catch (e) {
      console.log(`[WORKER] Network.enable failed for ${label}: ${e.message}`);
    }
  });

  cdp.on('Page.javascriptDialogOpening', () => cdp.send('Page.handleJavaScriptDialog', { accept: true }));

  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise(r => setTimeout(r, 20000));

  // Summary
  const allWS = [...wsMap.values()];
  console.log(`[METRIC] Total WS connections (main + workers): ${allWS.length}`);
  for (const ws of allWS) {
    console.log(`[METRIC]  WS: ${ws.url} sent:${ws.sent.length} recv:${ws.recv.length}`);
  }
  const { targetInfos } = await cdp.send('Target.getTargets', {});
  const wTargets = targetInfos.filter(t => ['worker','shared_worker','service_worker'].includes(t.type));
  console.log(`[METRIC] Worker targets: ${wTargets.length}`);
  for (const t of wTargets) console.log(`[METRIC]  type=${t.type} url=${t.url || '(blob)'}`);

  // Cleanup
  await cdp.send('ServiceWorker.disable', {});
  await cdp.send('Target.setAutoAttach', { autoAttach: false, waitForDebuggerOnStart: false, flatten: true });
  await cdp.send('Target.setDiscoverTargets', { discover: false });
}
```

**Key facts:**
- `flatten: true` on `setAutoAttach` is required — it makes all sessions share one WS connection and routes via `sessionId`
- Pass `sessionId` as the **third argument** to `cdp.send(method, params, sessionId)` to send commands to a worker session
- Worker `Network.*` events arrive on the main `cdp.on()` listener but carry a `meta.sessionId` in the second argument — check it to route correctly
- Workers spawned as `blob:` URLs cannot be read by source, but their network traffic (HTTP + WS) is fully capturable
- `Target.setAutoAttach` with `waitForDebuggerOnStart: false` lets workers run normally — set to `true` only if you need to pause a worker at startup for debugging
- `shared_worker` type means a single worker shared across tabs — it persists even after the page navigates
