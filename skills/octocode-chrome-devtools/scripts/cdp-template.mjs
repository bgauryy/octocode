// Base run(cdp) template. Use cdp-sandbox.mjs to run generated scripts.

export async function run(cdp) {

  // ── 0. SOURCE MAP RESOLVER (optional — must be first, before any navigation) ─
  // const { createSourceMapResolver } = await import(new URL('./sourcemap-resolver.mjs', import.meta.url).href);
  // const resolver = await createSourceMapResolver(cdp);

  // ── 1. ENABLE DOMAINS ────────────────────────────────────────────────────────
  await cdp.send('Runtime.enable', {});
  await cdp.send('Network.enable', {});
  await cdp.send('Log.enable', {});
  // await cdp.send('Page.enable', {});
  // await cdp.send('DOM.enable', {});
  // await cdp.send('CSS.enable', {});
  // await cdp.send('Performance.enable', {});

  console.log(`[METRIC] Inspecting: ${cdp.targetInfo.url}`);

  // ── 2. ATTACH EVENT LISTENERS ────────────────────────────────────────────────

  cdp.on('Runtime.consoleAPICalled', ({ type, args }) => {
    const msg = args.map(a => a.value ?? a.description ?? '[object]').join(' ');
    console.log(`[CONSOLE:${type.toUpperCase()}] ${msg}`);
  });

  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const desc = exceptionDetails.exception?.description ?? exceptionDetails.text;
    console.log(`[EXCEPTION] ${desc}`);
    const frame = exceptionDetails.stackTrace?.callFrames?.[0];
    if (frame)
      console.log(`[EXCEPTION_LOCATION] ${frame.url}:${frame.lineNumber}:${frame.columnNumber} in ${frame.functionName || '(anonymous)'}`);
  });

  const requests = new Map();
  cdp.on('Network.requestWillBeSent', ({ requestId, request }) => {
    requests.set(requestId, { url: request.url, method: request.method, start: Date.now() });
  });
  cdp.on('Network.responseReceived', ({ requestId, response }) => {
    const r = requests.get(requestId);
    if (!r) return;
    const duration = Date.now() - r.start;
    console.log(`[NETWORK] ${response.status} ${r.method} ${r.url} (${duration}ms)`);
    if (response.status >= 400) console.log(`[NETWORK_ERROR] HTTP ${response.status} → ${r.url}`);
    if (duration > 3000)       console.log(`[FINDING] SLOW_REQUEST: ${r.url} took ${duration}ms`);
  });
  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason }) => {
    const r = requests.get(requestId);
    console.log(`[NETWORK_FAILED] ${r?.url ?? 'unknown'}: ${blockedReason ? `blocked:${blockedReason}` : errorText}`);
  });

  cdp.on('Log.entryAdded', ({ entry }) => {
    if (entry.level === 'error' || entry.level === 'warning')
      console.log(`[LOG:${entry.level.toUpperCase()}] [${entry.source}] ${entry.text}${entry.url ? ` @ ${entry.url}:${entry.lineNumber}` : ''}`);
  });

  // ── 3. INSPECTION LOGIC ──────────────────────────────────────────────────────
  // Add your cdp.send() calls here. See SCRIPT_PATTERNS.md for copy-paste patterns.

  // ── 4. WAIT ───────────────────────────────────────────────────────────────────
  // Navigating inside run()? Use waitForNetworkIdle from SCRIPT_PATTERNS.md instead.
  const MONITOR_MS = 10000; // 3s static page · 10s dynamic · 30s long network check
  console.log(`[METRIC] Monitoring for ${MONITOR_MS / 1000}s...`);
  await new Promise(r => setTimeout(r, MONITOR_MS));

  console.log(`[METRIC] Total requests: ${requests.size}`);
}
