# CDP Observation Pattern Details

## Network Console (most common)

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

## DOM Accessibility Audit

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
    expression: `JSON.stringify(Object.entries(localStorage).map(([k,v]) => ({ k, size: new Blob([k+v]).size })))`, returnByValue: true,
  });
  const lsEntries = JSON.parse(ls.value ?? '[]');
  for (const { k } of lsEntries)
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
