# Script Patterns Reference

## Pattern Index

| Need | Pattern |
|---|---|
| Capture errors, failed requests, console output | [Network + Console](#network--console-most-common) |
| Measure high-level performance metrics | [Performance Audit](#performance-audit) |
| Measure LCP/CLS/INP with injected observers | [Core Web Vitals](#core-web-vitals-inject-before-navigate) |
| Inspect DOM size and accessibility basics | [DOM + Accessibility Audit](#dom--accessibility-audit) |
| Check heap usage and detached nodes | [Heap Memory Audit](#heap-memory-audit-leak-detection) |
| Audit headers, cookies, CSP, token exposure | [Security Audit](#security-audit) |
| Full cookie + localStorage + IDB + SW + quota inventory | `INTENTS.md` → `## storage` |
| Detect pre-granted GDPR consent and tracker pre-firing | `INTENTS.md` → `## consent` |
| Watch WebSocket frames | [WebSocket Surveillance](#websocket-surveillance) |
| Search all page resources for text | [Search Text Across All Resources](#search-text-across-all-resources) |
| Upload files through real inputs | [File Upload](#file-upload) |
| Wait for quiet network | [waitForNetworkIdle](#waitfornetworkidle) |
| Wait for usable elements | [waitForSelector with Actionability](#waitforselector-with-actionability) |
| Save screenshots, PDFs, and metadata | [Save Files](#save-files--screenshots-pdfs-and-metadata) |
| Query inside shadow roots | [Shadow DOM](#shadow-dom--querying-inside-shadow-roots) |
| Resolve generated locations to source maps | [Source Map Resolution](#source-map-resolution) |
| Combine major checks | [Full Audit](#full-audit-combine-all) |

## Network + Console (most common)

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Log.enable', {});

  const requests = new Map();

  cdp.on('Network.requestWillBeSent', ({ requestId, request }) => {
    requests.set(requestId, { url: request.url, method: request.method });
  });

  cdp.on('Network.responseReceived', ({ requestId, response }) => {
    const r = requests.get(requestId);
    if (!r) return;
    console.log(`[NETWORK] ${response.status} ${r.method} ${r.url}`);
    if (response.status >= 400)
      console.log(`[NETWORK_ERROR] HTTP ${response.status} → ${r.url}`);
  });

  cdp.on('Network.loadingFailed', ({ requestId, errorText }) => {
    const r = requests.get(requestId);
    console.log(`[NETWORK_FAILED] ${r?.url ?? 'unknown'}: ${errorText}`);
  });

  cdp.on('Runtime.consoleAPICalled', ({ type, args }) => {
    const msg = args.map(a => a.value ?? a.description ?? '[object]').join(' ');
    console.log(`[CONSOLE:${type.toUpperCase()}] ${msg}`);
  });

  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const desc = exceptionDetails.exception?.description ?? exceptionDetails.text;
    console.log(`[EXCEPTION] ${desc}`);
    if (exceptionDetails.stackTrace?.callFrames?.[0]) {
      const f = exceptionDetails.stackTrace.callFrames[0];
      console.log(`[EXCEPTION_LOCATION] ${f.url}:${f.lineNumber}:${f.columnNumber} in ${f.functionName}`);
    }
  });

  cdp.on('Log.entryAdded', ({ entry }) => {
    if (entry.level === 'error' || entry.level === 'warning')
      console.log(`[LOG:${entry.level.toUpperCase()}] [${entry.source}] ${entry.text}`);
  });

  // Navigate inside run() when monitoring load events; use --new-tab about:blank for that path
  await cdp.send('Page.enable', {});
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: true }));
  await cdp.send('Page.navigate', { url: 'https://example.com/' });

  console.log('[FINDING] Monitoring active — collecting events for 10s...');
  await new Promise(r => setTimeout(r, 10000));
  console.log(`[METRIC] Total requests captured: ${requests.size}`);
}
```

## Performance Audit

```js
export async function run(cdp) {
  await cdp.send('Performance.enable', {});
  await cdp.send('Runtime.enable', {});

  const { metrics } = await cdp.send('Performance.getMetrics', {});
  const m = Object.fromEntries(metrics.map(x => [x.name, x.value]));

  console.log('[PERFORMANCE] JSHeapUsedSize:', (m.JSHeapUsedSize / 1024 / 1024).toFixed(2), 'MB');
  console.log('[PERFORMANCE] TaskDuration:', m.TaskDuration?.toFixed(3), 's');
  console.log('[PERFORMANCE] LayoutCount:', m.LayoutCount);
  console.log('[PERFORMANCE] RecalcStyleCount:', m.RecalcStyleCount);
  console.log('[PERFORMANCE] ScriptDuration:', m.ScriptDuration?.toFixed(3), 's');
  console.log('[PERFORMANCE] Full metrics:', JSON.stringify(m, null, 2));

  if (m.JSHeapUsedSize > 50_000_000)
    console.log('[FINDING] HIGH_MEMORY: JS heap > 50MB — possible memory leak');
  if (m.LayoutCount > 20)
    console.log('[FINDING] LAYOUT_THRASHING: >20 forced layouts — check for read/write interleaving');
  if (m.ScriptDuration > 2)
    console.log('[FINDING] SLOW_SCRIPTS: script execution > 2s — profile for long tasks');
}
```

## Core Web Vitals (inject before navigate)

```js
// Add BEFORE Page.navigate — uses Page.addScriptToEvaluateOnNewDocument
await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
  source: `
    window.__CWV__ = {};
    new PerformanceObserver(list => {
      for (const e of list.getEntries())
        if (e.entryType === 'largest-contentful-paint') window.__CWV__.LCP = e.startTime;
    }).observe({ type: 'largest-contentful-paint', buffered: true });
    new PerformanceObserver(list => {
      for (const e of list.getEntries())
        if (e.entryType === 'layout-shift' && !e.hadRecentInput)
          window.__CWV__.CLS = (window.__CWV__.CLS || 0) + e.value;
    }).observe({ type: 'layout-shift', buffered: true });
    new PerformanceObserver(list => {
      for (const e of list.getEntries())
        if (e.name === 'first-contentful-paint') window.__CWV__.FCP = e.startTime;
    }).observe({ type: 'paint', buffered: true });
  `
});
// After page settles, read back:
const { result } = await cdp.send('Runtime.evaluate', { expression: `window.__CWV__`, returnByValue: true });
const cwv = result.value ?? {};
if (cwv.FCP) console.log(`[PERFORMANCE] FCP: ${cwv.FCP.toFixed(0)} ms [${cwv.FCP < 1800 ? 'GOOD' : 'POOR'}]`);
if (cwv.LCP) console.log(`[PERFORMANCE] LCP: ${cwv.LCP.toFixed(0)} ms [${cwv.LCP < 2500 ? 'GOOD' : 'POOR'}]`);
if (cwv.CLS != null) console.log(`[PERFORMANCE] CLS: ${cwv.CLS.toFixed(4)} [${cwv.CLS < 0.1 ? 'GOOD' : 'POOR'}]`);
```

## DOM + Accessibility Audit

```js
export async function run(cdp) {
  await cdp.send('DOM.enable', {});
  await cdp.send('Runtime.enable', {});

  const { root } = await cdp.send('DOM.getDocument', { depth: 2 });
  console.log(`[DOM] Root: ${root.nodeName}, children: ${root.childNodeCount}`);

  const checks = [
    ['Total elements',      `document.querySelectorAll('*').length`],
    ['Images missing alt',  `document.querySelectorAll('img:not([alt])').length`],
    ['Inputs missing label',`document.querySelectorAll('input:not([aria-label]):not([id])').length`],
    ['Empty buttons',       `document.querySelectorAll('button:empty').length`],
    ['Inline scripts',      `document.querySelectorAll('script:not([src])').length`],
  ];

  for (const [label, expr] of checks) {
    const { result } = await cdp.send('Runtime.evaluate', { expression: expr, returnByValue: true });
    console.log(`[DOM] ${label}: ${result.value}`);
    if (label === 'Total elements' && result.value > 1500)
      console.log('[FINDING] LARGE_DOM: >1500 elements — may hurt rendering performance');
    if (label === 'Images missing alt' && result.value > 0)
      console.log(`[FINDING] ACCESSIBILITY: ${result.value} images missing alt text`);
  }

  const { result: title } = await cdp.send('Runtime.evaluate', {
    expression: 'document.title', returnByValue: true,
  });
  console.log(`[DOM] Page title: "${title.value}"`);
}
```

## Heap Memory Audit (leak detection)

```js
export async function run(cdp) {
  await cdp.send('HeapProfiler.enable', {});

  const chunks = [];
  cdp.on('HeapProfiler.addHeapSnapshotChunk', ({ chunk }) => chunks.push(chunk));
  await cdp.send('HeapProfiler.takeHeapSnapshot', { reportProgress: false });

  const snapshot = JSON.parse(chunks.join(''));
  const { node_count, edge_count } = snapshot.snapshot.meta;
  console.log(`[METRIC] Heap nodes: ${node_count}, edges: ${edge_count}`);

  const strings = snapshot.strings;
  const nodeFields = snapshot.snapshot.meta.node_fields;
  const nodeSize  = nodeFields.length;
  const nodes     = snapshot.nodes;

  const typeCounts = {};
  for (let i = 0; i < nodes.length; i += nodeSize) {
    const name = strings[nodes[i + 1]];
    typeCounts[name] = (typeCounts[name] ?? 0) + nodes[i + 3];
  }

  const top = Object.entries(typeCounts).sort((a, b) => b[1] - a[1]).slice(0, 10);
  console.log('[PERFORMANCE] Top retained types by self_size:');
  for (const [name, size] of top) {
    console.log(`[PERFORMANCE]   ${name}: ${(size / 1024).toFixed(1)} KB`);
    if (size > 5_000_000)
      console.log(`[FINDING] HIGH_RETENTION: "${name}" retains ${(size / 1024 / 1024).toFixed(1)} MB — possible leak`);
  }

  const detachedIdx = nodeFields.indexOf('detachedness');
  if (detachedIdx !== -1) {
    let detached = 0;
    for (let i = 0; i < nodes.length; i += nodeSize)
      if (nodes[i + detachedIdx] === 1) detached++;
    console.log(`[METRIC] Detached DOM nodes: ${detached}`);
    if (detached > 50)
      console.log(`[FINDING] DETACHED_NODES: ${detached} detached DOM nodes — likely memory leak`);
  }
}
```

## Security Audit

```js
export async function run(cdp) {
  const TARGET_URL = 'https://example.com'; // ← set target URL

  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('DOM.enable', {});
  await cdp.send('Page.enable', {});

  const requests = new Map();

  cdp.on('Network.requestWillBeSent', ({ requestId, request }) => {
    requests.set(requestId, { url: request.url, method: request.method });
  });

  cdp.on('Network.responseReceived', async ({ requestId, response }) => {
    const r = requests.get(requestId);
    if (!r) return;
    const headers = response.headers ?? {};
    if (!headers['content-security-policy'])
      console.log(`[FINDING] MISSING_CSP: ${r.url}`);
    if (!headers['strict-transport-security'])
      console.log(`[FINDING] MISSING_HSTS: ${r.url}`);
    if (!headers['x-frame-options'] && !headers['content-security-policy']?.includes('frame-ancestors'))
      console.log(`[FINDING] MISSING_XFRAME: ${r.url}`);
    const csp = headers['content-security-policy'] ?? '';
    if (csp.includes('unsafe-eval')) console.log(`[FINDING] WEAK_CSP: unsafe-eval in ${r.url}`);
    if (csp.includes('unsafe-inline')) console.log(`[FINDING] WEAK_CSP: unsafe-inline in ${r.url}`);
    if (r.method === 'POST') {
      try {
        const { body } = await cdp.send('Network.getRequestPostData', { requestId });
        if (/token|password|secret|apikey|jwt|auth/.test(body.toLowerCase()))
          console.log(`[FINDING] SENSITIVE_IN_POST: ${r.url} — body contains sensitive key`);
      } catch {}
    }
  });

  // Use getCookies scoped to TARGET_URL — getAllCookies returns the entire browser jar
  // and floods output with third-party ad/tracker cookies irrelevant to the audited site
  const { cookies } = await cdp.send('Network.getCookies', { urls: [TARGET_URL] });
  for (const c of cookies) {
    if (!c.httpOnly) console.log(`[FINDING] COOKIE_NO_HTTPONLY: ${c.name}`);
    if (!c.secure)   console.log(`[FINDING] COOKIE_NO_SECURE: ${c.name}`);
    if (c.sameSite === 'None' && !c.secure)
      console.log(`[FINDING] COOKIE_SAMESITE_NONE_INSECURE: ${c.name}`);
  }
  console.log(`[SECURITY] Cookies audited: ${cookies.length}`);

  const { result: ls } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.entries(localStorage))`, returnByValue: true,
  });
  const lsEntries = JSON.parse(ls.value ?? '[]');
  for (const [k] of lsEntries)
    if (/token|auth|jwt|secret|key|password/i.test(k))
      console.log(`[FINDING] SENSITIVE_IN_STORAGE: localStorage key "${k}"`);
  console.log(`[SECURITY] localStorage keys: ${lsEntries.length}`);

  const { result: proto } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.keys(Object.getOwnPropertyDescriptors(Object.prototype)).filter(k => !['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'].includes(k)))`,
    returnByValue: true,
  });
  const polluted = JSON.parse(proto.value ?? '[]');
  if (polluted.length > 0)
    console.log(`[FINDING] PROTOTYPE_POLLUTION: unexpected keys on Object.prototype: ${polluted.join(', ')}`);

  const { result: docObj } = await cdp.send('Runtime.evaluate', { expression: 'document' });
  const { listeners } = await cdp.send('DOMDebugger.getEventListeners', { objectId: docObj.objectId });
  for (const l of listeners) {
    if (['keydown', 'keyup'].includes(l.type) && l.scriptId)
      console.log(`[FINDING] POSSIBLE_KEYLOGGER: document ${l.type} listener at ${l.scriptId}:${l.lineNumber}`);
    if (['copy', 'paste'].includes(l.type))
      console.log(`[FINDING] CLIPBOARD_LISTENER: document ${l.type} listener — possible hijack`);
  }

  await new Promise(r => setTimeout(r, 5000));
  console.log(`[METRIC] Security audit complete — requests: ${requests.size}, cookies: ${cookies.length}`);
}
```

## WebSocket Surveillance

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});

  const sockets = new Map();
  let frameCount = 0;

  cdp.on('Network.webSocketCreated', ({ requestId, url }) => {
    sockets.set(requestId, url);
    console.log(`[NETWORK] WS opened: ${url}`);
    try {
      const host = new URL(url).hostname;
      const pageHost = typeof location !== 'undefined' ? location.hostname : '';
      if (pageHost && host !== pageHost) console.log(`[FINDING] WS_UNKNOWN_HOST: ${url}`);
    } catch {}
  });

  cdp.on('Network.webSocketFrameSent', ({ requestId, response }) => {
    const url = sockets.get(requestId) ?? 'unknown';
    const payload = response.payloadData ?? '';
    frameCount++;
    console.log(`[NETWORK] WS SENT → ${url} (${payload.length} chars)`);
    if (/token|password|secret|key|auth/i.test(payload))
      console.log(`[FINDING] SENSITIVE_IN_WS_FRAME: sent to ${url}`);
    if (/^[A-Za-z0-9+/]{40,}={0,2}$/.test(payload.trim()))
      console.log(`[FINDING] WS_BASE64_FRAME: possible encoded data sent to ${url}`);
    if (payload.length > 100000)
      console.log(`[FINDING] LARGE_WS_FRAME: ${(payload.length / 1024).toFixed(1)}KB sent to ${url}`);
  });

  cdp.on('Network.webSocketFrameReceived', ({ requestId, response }) => {
    const url = sockets.get(requestId) ?? 'unknown';
    const payload = response.payloadData ?? '';
    frameCount++;
    console.log(`[NETWORK] WS RECV ← ${url} (${payload.length} chars)`);
  });

  cdp.on('Network.webSocketClosed', ({ requestId }) => {
    const url = sockets.get(requestId) ?? 'unknown';
    console.log(`[NETWORK] WS closed: ${url}`);
    sockets.delete(requestId);
  });

  console.log('[FINDING] WebSocket monitoring active — collecting for 15s...');
  await new Promise(r => setTimeout(r, 15000));
  console.log(`[METRIC] WS sockets seen: ${sockets.size + 1}  Total frames: ${frameCount}`);
}
```

## Search Text Across All Resources

```js
// Search for a string in all loaded JS, CSS, and network response bodies
// Requires Debugger.enable + Debugger.setSkipAllPauses (prevents breakpoint hangs)
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('DOM.enable', {});
  await cdp.send('CSS.enable', {});
  await cdp.send('Debugger.enable', {});
  await cdp.send('Debugger.setSkipAllPauses', { skip: true }); // CRITICAL: prevents Runtime.evaluate hangs

  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: true }));

  const SEARCH_TERM = 'YOUR_TERM_HERE';
  const scripts = {};
  const styleSheets = {};
  const responseIds = [];

  cdp.on('Debugger.scriptParsed', ({ scriptId, url }) => { scripts[scriptId] = url || '(inline)'; });
  cdp.on('CSS.styleSheetAdded', ({ header }) => { styleSheets[header.styleSheetId] = header.sourceURL || '(inline)'; });
  cdp.on('Network.responseReceived', ({ requestId, response }) => {
    const ct = (response.mimeType ?? '').toLowerCase();
    if (['json','text','html','javascript','xml','css'].some(t => ct.includes(t)))
      responseIds.push([requestId, response.url]);
  });

  await cdp.send('Page.navigate', { url: 'https://TARGET_URL/' });
  await new Promise(r => setTimeout(r, 4000));

  // Search in JS
  for (const [scriptId, url] of Object.entries(scripts)) {
    let result;
    try { ({ result } = await cdp.send('Debugger.searchInContent', {
      scriptId, query: SEARCH_TERM, caseSensitive: false, isRegex: false
    })); } catch { continue; }
    if (result?.length)
      result.forEach(r => console.log(`[SEARCH] JS L${r.lineNumber}: ${r.lineContent.trim().slice(0,120)}`));
  }

  // Search in CSS
  for (const [styleSheetId] of Object.entries(styleSheets)) {
    let text;
    try { ({ text } = await cdp.send('CSS.getStyleSheetText', { styleSheetId })); } catch { continue; }
    if (text?.toLowerCase().includes(SEARCH_TERM.toLowerCase()))
      console.log(`[SEARCH] CSS hit found`);
  }

  // Search in network bodies
  for (const [requestId, url] of responseIds) {
    try {
      const { body, base64Encoded } = await cdp.send('Network.getResponseBody', { requestId });
      const text = base64Encoded ? Buffer.from(body, 'base64').toString() : (body ?? '');
      if (text.toLowerCase().includes(SEARCH_TERM.toLowerCase()))
        console.log(`[SEARCH] BODY hit in ${url.split('/').pop() || url}`);
    } catch { continue; }
  }

  // Search in DOM text
  const { result: domRes } = await cdp.send('Runtime.evaluate', {
    expression: `(function() {
      const term = '${SEARCH_TERM.toLowerCase()}';
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
      const hits = []; let node;
      while ((node = walker.nextNode()))
        if (node.textContent.toLowerCase().includes(term))
          hits.push('<' + node.parentElement?.tagName + '> ' + node.textContent.trim().slice(0,80));
      return hits;
    })()`, returnByValue: true
  });
  (domRes.value ?? []).forEach(h => console.log(`[SEARCH] DOM: ${h}`));
}
```

## File Upload

Upload a file via a native `input[type="file"]` element using `DOM.setFileInputFiles`. Files must be absolute paths on the machine running Chrome.

```js
export async function run(cdp) {
  await cdp.send('DOM.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});

  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: true }));

  await cdp.send('Page.navigate', { url: 'https://example.com/upload' });
  await new Promise(r => setTimeout(r, 2000));

  // Find the file input
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeId } = await cdp.send('DOM.querySelector', {
    nodeId: root.nodeId,
    selector: 'input[type="file"]',
  });

  if (nodeId === 0) {
    console.log('[FINDING] NO_FILE_INPUT: no input[type="file"] found on page');
    return;
  }

  // Set files — absolute paths only
  await cdp.send('DOM.setFileInputFiles', {
    nodeId,
    files: ['/absolute/path/to/your-file.txt'],
    // For multiple files: files: ['/path/a.txt', '/path/b.png']
  });
  console.log('[AUTOMATE] file set on input[type="file"]');

  // Dispatch change + input events so React/Vue/Angular frameworks detect the selection
  await cdp.send('Runtime.evaluate', {
    expression: `
      const el = document.querySelector('input[type="file"]');
      el.dispatchEvent(new Event('input',  { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    `,
  });
  console.log('[AUTOMATE] change/input events dispatched');

  await new Promise(r => setTimeout(r, 1500));
  console.log('[METRIC] File upload step complete');
}
```

**Gotchas:**
- `files` must be **absolute paths** — relative paths and URLs are rejected
- `multiple` inputs: pass all files in one array `files: ['/a', '/b']`
- Hidden file inputs triggered by a button: click the button first via `Runtime.evaluate` click, then call `DOM.setFileInputFiles` on the now-visible (or still-hidden) `nodeId`
- Always dispatch `change` and `input` events after setting files — CDP sets the value silently, frameworks won't react otherwise
- If the nodeId is 0 and you know the input exists, the page may still be loading — add a `waitForSelector` call before `DOM.querySelector`


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


## Save Files — Screenshots, PDFs, and Metadata

Always use `cdp.outputDir` — it is the only writable location in sandbox mode and works on Windows, macOS, and Linux.
Output lands in `<TMPDIR>/.octocode-chrome-devtools/<timestamp>/` — the agent reads the `[CDP_RUNNER] Output dir:` line in stderr to find it.

```js
export async function run(cdp) {
  await cdp.send('Page.enable', {});

  const { writeFileSync } = await import('fs');
  const { join } = await import('path');

  // ── Screenshot ────────────────────────────────────────────────────────────
  await cdp.send('Page.navigate', { url: 'https://example.com' });
  await new Promise(r => setTimeout(r, 2000)); // wait for render

  const { data: pngData } = await cdp.send('Page.captureScreenshot', { format: 'png' });
  const screenshotPath = join(cdp.outputDir, 'screenshot.png');
  writeFileSync(screenshotPath, Buffer.from(pngData, 'base64'));
  console.log(`[SCREENSHOT] ${screenshotPath}`);

  // ── PDF ───────────────────────────────────────────────────────────────────
  const { data: pdfData } = await cdp.send('Page.printToPDF', { printBackground: true });
  const pdfPath = join(cdp.outputDir, 'page.pdf');
  writeFileSync(pdfPath, Buffer.from(pdfData, 'base64'));
  console.log(`[FINDING] PDF saved → ${pdfPath}`);

  // ── Metadata / findings JSON ──────────────────────────────────────────────
  const metadata = {
    url:       cdp.targetInfo.url,
    timestamp: new Date().toISOString(),
    findings:  [],  // push [FINDING] items here to get a machine-readable report
  };
  // metadata.findings.push({ type: 'HTTP_ERROR', status: 404, url: '...' });
  const metaPath = join(cdp.outputDir, 'metadata.json');
  writeFileSync(metaPath, JSON.stringify(metadata, null, 2));
  console.log(`[METRIC] metadata saved → ${metaPath}`);
}
```

**Key rules:**
- Use `cdp.outputDir` — never `os.tmpdir()` directly in sandbox mode
- Write pattern: `const { writeFileSync } = await import('fs'); const { join } = await import('path');`
- The runner logs `[CDP_RUNNER] Output dir: <path>` to stderr — the agent reads this to locate all output files


## Shadow DOM — Querying Inside Shadow Roots

`DOM.querySelector / querySelectorAll` do **not** pierce shadow boundaries. Use `Runtime.evaluate` with a recursive traversal, or `DOM.getDocument({ pierce: true })` to inspect the full tree.

```js
// Requires: DOM.enable, Runtime.enable

// Returns a remote objectId — use with Runtime.callFunctionOn or DOM.resolveNode
async function queryShadowDOM(cdp, selector) {
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `(function pierce(root, sel) {
      const el = root.querySelector(sel);
      if (el) return el;
      for (const host of root.querySelectorAll('*')) {
        if (host.shadowRoot) {
          const found = pierce(host.shadowRoot, sel);
          if (found) return found;
        }
      }
      return null;
    })(document, ${JSON.stringify(selector)})`,
    returnByValue: false, // keep remote objectId for further CDP calls
  });
  return result.objectId ?? null;
}

export async function run(cdp) {
  await cdp.send('DOM.enable', {});
  await cdp.send('Runtime.enable', {});

  // Get document tree including shadow roots (pierce: true)
  // Shadow roots appear as DOCUMENT_FRAGMENT nodes (nodeType 11) in the children array
  const { root } = await cdp.send('DOM.getDocument', { depth: 3, pierce: true });
  console.log(`[DOM] Root: ${root.nodeName}, childCount: ${root.childNodeCount}`);

  // Extract all text from elements inside shadow roots via Runtime.evaluate
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `(function() {
      const hits = [];
      function walk(root) {
        for (const el of root.querySelectorAll('*')) {
          if (el.shadowRoot) walk(el.shadowRoot);
        }
        // change 'my-component button' to your target selector
        for (const el of root.querySelectorAll('my-component button')) {
          hits.push(el.textContent.trim());
        }
      }
      walk(document);
      return JSON.stringify(hits);
    })()`,
    returnByValue: true,
  });
  const items = JSON.parse(result.value ?? '[]');
  items.forEach(t => console.log(`[SCRAPE] shadow-DOM item: "${t}"`));
  console.log(`[METRIC] Shadow DOM items found: ${items.length}`);
  if (items.length === 0)
    console.log('[FINDING] SHADOW_DOM_EMPTY: selector found nothing in shadow roots — check host element and inner selector');
}

async function clickInShadowDOM(cdp, hostSelector, innerSelector) {
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `(function() {
      const host = document.querySelector(${JSON.stringify(hostSelector)});
      const el   = host?.shadowRoot?.querySelector(${JSON.stringify(innerSelector)});
      if (!el) return { found: false };
      const r = el.getBoundingClientRect();
      return { found: true, x: r.left + r.width / 2, y: r.top + r.height / 2 };
    })()`,
    returnByValue: true,
  });
  if (!result.value?.found) {
    console.log(`[FINDING] SHADOW_DOM_NOT_FOUND: "${innerSelector}" not found in "${hostSelector}" shadow root`);
    return;
  }
  const { x, y } = result.value;
  await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  console.log(`[AUTOMATE] clicked "${innerSelector}" inside "${hostSelector}" shadow root`);
}
```

**Key facts:**
- `DOM.getDocument({ pierce: true })` includes shadow roots in the returned tree — shadow roots are `nodeType: 11` (DOCUMENT_FRAGMENT_NODE)
- `DOM.querySelector / querySelectorAll` do **NOT** cross shadow boundaries even with `pierce: true` on the document
- `Runtime.evaluate` with recursive traversal is the most reliable approach for querying
- **Closed shadow roots** (`attachShadow({ mode: 'closed' })`) — JavaScript cannot access `.shadowRoot`; CDP has no bypass
- **Nested shadows** — the `pierce()` helper above handles arbitrary nesting depth


## Source Map Resolution

Resolves minified compiled positions back to original source names and files.
Requires `sourcemap-resolver.mjs` to be present in the same directory as the script.
The sandbox runner stages `sourcemap-resolver.mjs` in `$TMPDIR` automatically for generated scripts.
Works gracefully when maps are absent — always returns `null` instead of throwing.

**When to add this pattern:**
- `js-coverage` intent — show readable function names in DEAD_CODE findings
- `debug` intent — enrich stack frames with original file + line
- Any intent where you want to understand *what* a minified script does

```js
// Import the resolver BEFORE enabling other domains (must register scriptParsed ASAP)
const { createSourceMapResolver } = await import(
  new URL('./sourcemap-resolver.mjs', import.meta.url).href
);
const resolver = await createSourceMapResolver(cdp);
// Debugger.enable and Debugger.setSkipAllPauses are called internally by createSourceMapResolver

// ...enable Network, Profiler, etc. and navigate the page...

// After page is fully loaded, wait for all map loads to settle:
await resolver.settle();


// Option A: resolve a single generated position
const orig = resolver.resolve(scriptId, lineNumber, columnNumber); // all 0-indexed
if (orig) {
  const fnName = orig.name ?? '(anonymous)';
  const src    = orig.source?.split('/').slice(-2).join('/') ?? 'unknown'; // last 2 path parts
  console.log(`[SOURCEMAP] ${fnName} → ${src}:${orig.line}`);
}

// Option B: enrich Profiler coverage results with source map data
for (const script of coverageResult) {
  const url = script.url;
  if (!url || url.startsWith('chrome-extension')) continue;

  for (const fn of script.functions) {
    const isUsed = fn.ranges.some(r => r.count > 0);
    if (isUsed) continue; // only report dead code

    // Try to resolve the first byte of the function
    const [startLine, startCol] = offsetToLineCol(fn.ranges[0]?.startOffset ?? 0, compiledText);
    const orig = resolver.resolve(script.scriptId, startLine, startCol);

    const displayName = orig?.name ?? (fn.functionName?.length > 2 ? fn.functionName : null);
    if (!displayName) continue; // skip mangled single/double chars

    const loc = orig
      ? `${orig.source?.split('/').pop() ?? 'unknown'}:${orig.line}`
      : url.split('/').pop();
    console.log(`[FINDING] DEAD_CODE: ${displayName} in ${loc}`);
  }
}

// Helper: convert character offset to {line, col} (0-indexed) — only needed for Profiler
function offsetToLineCol(offset, source) {
  let line = 0, col = 0;
  for (let i = 0; i < offset && i < source.length; i++) {
    if (source[i] === '\n') { line++; col = 0; } else col++;
  }
  return [line, col];
}

resolver.printSummary();
// Emits: [SOURCEMAP] 42 scripts: 12 maps loaded, 3 failed, 27 had no map
```

**Key facts:**
- `Debugger.scriptParsed` fires for every script during page load — resolver must be created before navigation
- `resolver.settle()` must be called after page is loaded to ensure all async map fetches complete
- Inline `data:application/json;base64,...` maps are decoded instantly with no network call
- External `.map` URLs are fetched with a 4s timeout; failures are silently counted
- `sourcesContent` (full original source code) is **always stripped** — never stored or emitted
- `resolver.resolve()` returns `null` when script has no map or position is outside all segments
- The Profiler gives `startOffset` (byte offset) not `{line,col}` — use `offsetToLineCol()` helper
- Short function names (`length <= 2`) after mangling are meaningless — skip unless `orig.name` resolves them



## Storage Audit

> **Full script in `INTENTS.md` → `## storage`** — grep: `rg -n "^## storage" references/INTENTS.md`

Inventories cookies, localStorage, sessionStorage, IndexedDB, Cache Storage, Service Workers, and quota.
Also detects **cookie resurrection** (tracking IDs mirrored across storage to survive clearing).


## Consent Audit

> **Full script in `INTENTS.md` → `## consent`** — grep: `rg -n "^## consent" references/INTENTS.md`

Detects CMP presence, pre-granted consent state, and tracker firing before user consent.


## Full Audit (combine all)

Combine the Network + Console, Performance Audit, DOM + Accessibility, and Security Audit `run()` functions above into a single script — enable all required domains at the top, attach all event listeners before navigating, run all sync checks after the page settles.
