# CDP Intent Reference

## Combining Intents

Intents are composable. Most real tasks need 2–3 combined. Enable the **union of domains** for all selected intents and merge their event listeners.

| Common combination | Intents | When to use |
|---|---|---|
| Automate + debug | `automate` + `debug` | "do X and tell me what breaks" |
| Login + network | `login` + `network` | "log in and capture what API calls are made" |
| Login + security | `login` + `security` | "log in and audit cookies/tokens after auth" |
| User-auth + scrape | `user-auth` + `scrape` | "let me sign in, then extract data from the authenticated page" |
| User-auth + debug | `user-auth` + `debug` | "let me sign in, then investigate what's breaking after auth" |
| User-auth + security | `user-auth` + `security` | "let me sign in, then audit the session tokens and cookies" |
| Automate + screenshot | `automate` + `screenshot` | "fill the form and take a screenshot of the result" |
| Automate + performance | `automate` + `performance` | "click through the flow and measure render time" |
| Scrape + emulate | `scrape` + `emulate` | "extract data as mobile viewport" |
| Debug + security | `debug` + `security` | "investigate the crash and check what tokens are exposed" |
| Inject + debug | `inject` + `debug` | "patch the function before load, then observe behavior" |
| Full audit | all of the above | "audit everything" |

When combining: enable all required domains first, attach all event listeners, then navigate/interact, then run inspection checks.


## Intent Index

| User says… | Intent | Jump to |
|---|---|---|
| debug, what's wrong, broken, fix this, investigate, agent loop, observe, why is X not working | [debug](#debug) | ↓ |
| automate, do X, click, type, fill, submit, flow, interact, perform steps | [automate](#automate) | ↓ |
| login, sign in, authenticate, enter credentials, log me in | [login](#login) | ↓ |
| let me log in myself, I'll auth, manual login, open browser so I can sign in, auth flow, open visible browser, I need to authenticate first | [user-auth](#user-auth) | ↓ |
| scrape, extract, collect data, pull content, harvest | [scrape](#scrape) | ↓ |
| emulate, mobile, device, throttle, offline, slow network, geolocation | [emulate](#emulate) | ↓ |
| inject, patch, override, hook, intercept before load, monkey-patch | [inject](#inject) | ↓ |
| monitor, watch, poll, check every N seconds, keep watching | [monitor](#monitor) | ↓ |
| network, requests, 4xx, API calls, traffic | [network](#network) | ↓ |
| console, errors, exceptions, crashes, JS error | [console](#console) | ↓ |
| slow, performance, metrics, long task, fps, render | [performance](#performance) | ↓ |
| memory, leak, heap, detached nodes, retained | [memory](#memory) | ↓ |
| DOM, elements, structure, HTML, rendering | [dom](#dom) | ↓ |
| CSS, styles, unused rules, coverage | [css-coverage](#css-coverage) | ↓ |
| JS coverage, dead code, unused functions | [js-coverage](#js-coverage) | ↓ |
| security, cookies, tokens, headers, CSP, exfil | [security](#security) | ↓ |
| websocket, WS, real-time, socket frames | [websocket](#websocket) | ↓ |
| intercept, mock, block, fake response, modify request | [intercept](#intercept) | ↓ |
| screenshot, capture, visual, PDF, print | [screenshot](#screenshot) | ↓ |
| accessibility, a11y, aria, screen reader | [accessibility](#accessibility) | ↓ |
| third-party, external scripts, CDN, supply chain | [supply-chain](#supply-chain) | ↓ |
| full audit, all checks, everything | [full-audit](#full-audit) | ↓ |


## debug

**Trigger phrases:** "debug", "what's wrong", "why is X broken", "investigate", "fix this", "something is not working", "help me understand", "agent loop", "observe", "what happened", "trace this", "why does X fail"

**Purpose:** Designed for iterative **Reason → Observe → Act** loops by developers and agents. Captures maximum signal in a single pass — errors, failed requests, DOM state, and source locations — so the next action is usually clear.

**Domains:** `Network.enable`, `Runtime.enable`, `Log.enable`, `DOM.enable`, `Page.enable`, `Debugger.enable` (auto-enabled by `createSourceMapResolver` — enrich minified stack frames)

**Key events/methods:**
- `Runtime.exceptionThrown` → full stack trace with `url:line:col:functionName`
- `Runtime.consoleAPICalled` → filter for `error` and `warn` types only
- `Log.entryAdded` → all `error`/`warning` entries with source
- `Network.requestWillBeSent` → capture URL + method
- `Network.responseReceived` → flag status ≥ 400
- `Network.loadingFailed` → blocked/failed requests
- `Network.getRequestPostData(requestId)` → POST body on failed requests (call inside response handler)
- `DOM.getDocument(depth:1)` → confirm page loaded (root title, body present)
- `Runtime.evaluate` → `document.title`, `document.readyState`, `document.querySelectorAll('.error, [data-error], [aria-invalid]').length`

**Output prefixes:** `[DEBUG]` `[EXCEPTION]` `[EXCEPTION_LOCATION]` `[CONSOLE:ERROR]` `[NETWORK_ERROR]` `[NETWORK_FAILED]` `[DOM]` `[ACTION]` `[METRIC]` `[FINDING]`

**Additional prefix — `[ACTION]`:** Emitted when the agent can determine a concrete next step from a finding. Format: `[ACTION] <verb> <target> — <reason>`. Example: `[ACTION] search "TypeError: Cannot read" in localSearchCode — exception at checkout.js:42`.

**Agent loop contract:**

Each debug run produces one **OBSERVE block** and one **ACT block**:

```
[DEBUG] === OBSERVE ===
[DEBUG] Page: <title> | readyState: <state>
[DEBUG] Exceptions: N  Console errors: N  Network errors: N  Blocked: N
... (all [EXCEPTION], [NETWORK_ERROR], [NETWORK_FAILED] lines) ...
[DEBUG] DOM error indicators: N elements with .error / [aria-invalid]

[DEBUG] === ACT ===
[ACTION] <highest-priority action based on findings>
[ACTION] <second action if applicable>
```

The ACT block is what the agent (or developer) should do next. Emit it when there is a concrete next move; if findings are zero, one concise `[ACTION] No errors found — try interacting with the page and re-run` line is enough.

**`[FINDING]` conditions to emit (in priority order):**

| Priority | Condition | Finding |
|---|---|---|
| 1 | Uncaught exception with stack trace | `[FINDING] EXCEPTION: ${description} at ${url}:${line}` |
| 2 | Network request ≥ 400 | `[FINDING] HTTP_ERROR: ${status} ${method} ${url}` |
| 3 | Request blocked / failed | `[FINDING] BLOCKED: ${url} — ${errorText}` |
| 4 | `console.error` message | `[FINDING] CONSOLE_ERROR: ${message}` |
| 5 | DOM contains `.error`, `[aria-invalid]`, `[data-error]` elements | `[FINDING] DOM_ERROR_STATE: ${n} error-state elements visible` |
| 6 | `document.readyState !== 'complete'` after load wait | `[FINDING] PAGE_NOT_READY: readyState=${state}` |
| 7 | Zero network requests after navigation | `[FINDING] NO_REQUESTS: page may be offline or blocked` |

**`[ACTION]` emit rules:**

- For every `[FINDING] EXCEPTION` → `[ACTION] localSearchCode "${functionName}" to find source at ${url}:${line}`
- For every `[FINDING] HTTP_ERROR` → `[ACTION] check handler for ${method} ${url} — returned ${status}`
- For every `[FINDING] BLOCKED` → `[ACTION] check CORS / network config for ${url}`
- For every `[FINDING] CONSOLE_ERROR` → `[ACTION] search "${first 60 chars of message}" in localSearchCode`
- For every `[FINDING] DOM_ERROR_STATE` → `[ACTION] inspect DOM for .error / [aria-invalid] — user-visible errors present`
- If zero findings → `[ACTION] No errors detected — interact with the page (click, submit form) and re-run debug`

**Pre-built debug script:**

```js
export async function run(cdp) {
  const TARGET_URL = 'https://example.com'; // ← set the URL to debug; use --new-tab about:blank

  // Source map resolver — enables Debugger internally, must be first
  const { createSourceMapResolver } = await import(
    new URL('./sourcemap-resolver.mjs', import.meta.url).href
  );
  const resolver = await createSourceMapResolver(cdp);

  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Log.enable', {});
  await cdp.send('DOM.enable', {});
  await cdp.send('Page.enable', {});

  const errors = { exceptions: [], consoleErrors: [], networkErrors: [], blocked: [] };
  const requests = new Map();

  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const desc = exceptionDetails.exception?.description ?? exceptionDetails.text ?? 'Unknown error';
    const frame = exceptionDetails.stackTrace?.callFrames?.[0];
    errors.exceptions.push({ desc, frame });
    console.log(`[EXCEPTION] ${desc}`);
    if (frame) {
      // Try to resolve minified location to original source
      const orig = resolver.resolve(frame.scriptId, frame.lineNumber, frame.columnNumber);
      const fnName = (orig?.name ?? frame.functionName) || '(anonymous)';
      const loc = orig
        ? `${orig.source?.split('/').slice(-2).join('/') ?? frame.url}:${orig.line} [${frame.url}:${frame.lineNumber}]`
        : `${frame.url}:${frame.lineNumber}:${frame.columnNumber}`;
      console.log(`[EXCEPTION_LOCATION] ${loc} in ${fnName}`);
    }
  });

  cdp.on('Runtime.consoleAPICalled', ({ type, args }) => {
    if (type !== 'error' && type !== 'warn') return;
    const msg = args.map(a => a.value ?? a.description ?? '[object]').join(' ');
    errors.consoleErrors.push(msg);
    console.log(`[CONSOLE:${type.toUpperCase()}] ${msg}`);
  });

  cdp.on('Log.entryAdded', ({ entry }) => {
    if (entry.level !== 'error' && entry.level !== 'warning') return;
    console.log(`[LOG:${entry.level.toUpperCase()}] [${entry.source}] ${entry.text}`);
  });

  cdp.on('Network.requestWillBeSent', ({ requestId, request }) => {
    requests.set(requestId, { url: request.url, method: request.method });
  });

  cdp.on('Network.responseReceived', async ({ requestId, response }) => {
    const r = requests.get(requestId);
    if (!r) return;
    if (response.status >= 400) {
      errors.networkErrors.push({ status: response.status, ...r });
      console.log(`[NETWORK_ERROR] HTTP ${response.status} ${r.method} ${r.url}`);
      console.log(`[FINDING] HTTP_ERROR: ${response.status} ${r.method} ${r.url}`);
      if (r.method === 'POST') {
        try {
          const { body } = await cdp.send('Network.getRequestPostData', { requestId });
          console.log(`[DEBUG] POST body (first 200 chars): ${body.slice(0, 200)}`);
        } catch {}
      }
    }
  });

  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason }) => {
    const r = requests.get(requestId);
    if (!r) return;
    errors.blocked.push({ url: r.url, errorText });
    console.log(`[NETWORK_FAILED] ${r.url}: ${errorText}${blockedReason ? ` (blocked: ${blockedReason})` : ''}`);
    console.log(`[FINDING] BLOCKED: ${r.url} — ${errorText}`);
  });

  // Navigate AFTER all listeners are attached (event ordering rule)
  // Replace TARGET_URL with the URL to debug — use --new-tab about:blank so events aren't missed
  await cdp.send('Page.navigate', { url: TARGET_URL });

  // Wait for page activity and source map loads
  await new Promise(r => setTimeout(r, 8000));
  await resolver.settle();

  // DOM state snapshot
  const { result: title } = await cdp.send('Runtime.evaluate', { expression: 'document.title', returnByValue: true });
  const { result: readyState } = await cdp.send('Runtime.evaluate', { expression: 'document.readyState', returnByValue: true });
  const { result: errorEls } = await cdp.send('Runtime.evaluate', {
    expression: `document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length`,
    returnByValue: true,
  });

  console.log(`[DOM] Page: "${title.value}" | readyState: ${readyState.value}`);
  if (errorEls.value > 0) {
    console.log(`[FINDING] DOM_ERROR_STATE: ${errorEls.value} error-state elements visible`);
    console.log(`[ACTION] inspect DOM for .error / [aria-invalid] — user-visible errors present`);
  }
  if (readyState.value !== 'complete')
    console.log(`[FINDING] PAGE_NOT_READY: readyState=${readyState.value}`);

  // OBSERVE summary
  console.log(`\n[DEBUG] === OBSERVE ===`);
  console.log(`[DEBUG] Page: "${title.value}" | readyState: ${readyState.value}`);
  console.log(`[DEBUG] Exceptions: ${errors.exceptions.length}  Console errors: ${errors.consoleErrors.length}  Network errors: ${errors.networkErrors.length}  Blocked: ${errors.blocked.length}`);
  console.log(`[DEBUG] DOM error indicators: ${errorEls.value} elements`);

  // ACT block — emit concrete next steps
  console.log(`\n[DEBUG] === ACT ===`);
  let hasActions = false;

  for (const { desc, frame } of errors.exceptions) {
    const loc = frame ? `${frame.url}:${frame.lineNumber}` : 'unknown location';
    const fn = frame?.functionName || '(anonymous)';
    console.log(`[ACTION] localSearchCode "${fn !== '(anonymous)' ? fn : desc.slice(0, 50)}" — exception at ${loc}`);
    hasActions = true;
  }
  for (const { status, method, url } of errors.networkErrors) {
    console.log(`[ACTION] check handler for ${method} ${url} — returned ${status}`);
    hasActions = true;
  }
  for (const { url, errorText } of errors.blocked) {
    console.log(`[ACTION] check CORS / network config for ${url} — ${errorText}`);
    hasActions = true;
  }
  for (const msg of errors.consoleErrors.slice(0, 3)) {
    console.log(`[ACTION] localSearchCode "${msg.slice(0, 60)}" — console error`);
    hasActions = true;
  }
  if (!hasActions)
    console.log(`[ACTION] No errors detected — interact with the page (click, submit form) and re-run debug`);

  console.log(`\n[METRIC] Total requests: ${requests.size}`);
  resolver.printSummary();
}
```

**Agent loop usage:**

```
REASON   → what do I expect to see?
OBSERVE  → run debug script → read [FINDING] + [EXCEPTION] + [NETWORK_ERROR] lines
ACT      → follow [ACTION] lines: localSearchCode, check handler, fix CORS, inspect DOM
REPEAT   → re-run after fix to confirm errors cleared
```

Stop the loop when: zero `[FINDING]` lines **and** `readyState: complete` **and** `DOM error indicators: 0`.


## network

**Trigger phrases:** "check network", "API calls", "requests", "4xx", "5xx", "traffic", "what's being called", "HTTP errors"

**Domains:** `Network.enable`

**Key events/methods:**
- `Network.requestWillBeSent` → capture URL, method, request headers
- `Network.responseReceived` → capture status code
- `Network.loadingFailed` → capture blocked/failed requests
- `Network.getRequestPostData(requestId)` → capture actual POST body (call inside `responseReceived`)
- `Network.getResponseBody(requestId)` → capture response body when needed

**Output prefixes:** `[NETWORK]` `[NETWORK_ERROR]` `[NETWORK_FAILED]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- Status ≥ 400 → `[FINDING] HTTP_ERROR: ${status} ${method} ${url}`
- `loadingFailed` → `[FINDING] REQUEST_BLOCKED: ${url} — ${errorText}`
- POST body contains `token`, `password`, `secret`, `key` → `[FINDING] SENSITIVE_DATA_IN_REQUEST: ${url}`
- Request to unexpected third-party domain → `[FINDING] THIRD_PARTY_REQUEST: ${url}`

**Pattern:** Use `Network + Console` pattern from `SCRIPT_PATTERNS.md`, remove console section if not needed.


## console

**Trigger phrases:** "console errors", "JS errors", "exceptions", "crashes", "what's broken", "runtime errors"

**Domains:** `Runtime.enable`, `Log.enable`

**Key events/methods:**
- `Runtime.consoleAPICalled` → all `console.*` calls
- `Runtime.exceptionThrown` → uncaught exceptions with stack trace
- `Log.entryAdded` → browser-level log entries (network errors surfaced here too)

**Output prefixes:** `[CONSOLE:ERROR]` `[CONSOLE:WARN]` `[EXCEPTION]` `[EXCEPTION_LOCATION]` `[LOG:ERROR]`

**`[FINDING]` conditions to emit:**
- Any `[EXCEPTION]` → `[FINDING] UNCAUGHT_EXCEPTION: ${description}`
- `[CONSOLE:ERROR]` count > 0 → `[FINDING] CONSOLE_ERRORS: ${count} errors found`
- `[LOG:ERROR]` from `security` source → `[FINDING] SECURITY_LOG_ERROR: ${text}`
- Stack frame points to third-party URL → `[FINDING] THIRD_PARTY_EXCEPTION: ${url}`

**Pattern:** Use `Network + Console` pattern from `SCRIPT_PATTERNS.md`, remove network section if not needed.


## performance

**Trigger phrases:** "slow", "performance", "metrics", "long tasks", "layout thrashing", "FPS", "script duration", "rendering", "CPU"

**Domains:** `Performance.enable`, `Runtime.enable`

**Key events/methods:**
- `Performance.getMetrics` → JSHeapUsedSize, TaskDuration, ScriptDuration, LayoutCount, RecalcStyleCount, Nodes
- `Tracing.start` / `Tracing.end` + `Tracing.dataCollected` → full timeline trace (use for deep profiling)

**Output prefixes:** `[PERFORMANCE]` `[METRIC]` `[FINDING]`

**`[FINDING]` conditions to emit:**
- `JSHeapUsedSize > 50MB` → `[FINDING] HIGH_MEMORY: JS heap ${MB}MB`
- `ScriptDuration > 2s` → `[FINDING] SLOW_SCRIPTS: ${s}s script execution`
- `LayoutCount > 20` → `[FINDING] LAYOUT_THRASHING: ${n} forced layouts`
- `RecalcStyleCount > 30` → `[FINDING] STYLE_RECALC: ${n} style recalculations`
- `Nodes > 1500` → `[FINDING] LARGE_DOM: ${n} DOM nodes`

**Pattern:** Use `Performance Audit` pattern from `SCRIPT_PATTERNS.md`.

**Cold-load accuracy — do this before navigating for first-visit metrics:**
```js
// Ensure cold-cache metrics (real TTFB, resource timing, FCP)
await cdp.send('Network.clearBrowserCache', {});
await cdp.send('Network.clearBrowserCookies', {});
// Then navigate — DNS/TCP/TLS/FCP will now reflect a real first visit
```
Without this, a second navigation reuses the HTTP cache and connection pool, making DNS = 0ms, TCP = 0ms, all resource durations = 0ms, and FCP = null.

**FCP — two approaches (pick one):**

*Approach A (recommended) — read from the page's own performance timeline after load:*
```js
const { result } = await cdp.send('Runtime.evaluate', {
  expression: `JSON.stringify({
    fcp: performance.getEntriesByType('paint').find(e => e.name === 'first-contentful-paint')?.startTime,
    lcp: performance.getEntriesByType('largest-contentful-paint').slice(-1)[0]?.startTime,
  })`,
  returnByValue: true,
});
const { fcp, lcp } = JSON.parse(result.value || '{}');
if (fcp != null) console.log(`[METRIC] FCP: ${Math.round(fcp)}ms`);
if (lcp != null) console.log(`[METRIC] LCP: ${Math.round(lcp)}ms`);
// Values are already in ms from navigationStart — no reference-frame conversion needed
```

*Approach B — CDP lifecycle events (needs `Page.setLifecycleEventsEnabled`):*
```js
await cdp.send('Page.enable', {});
await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true }); // required!
let navStartTs = null;
cdp.on('Page.lifecycleEvent', ({ name, timestamp }) => {
  if (name === 'commit') navStartTs = timestamp;
  if (name === 'firstContentfulPaint' && navStartTs !== null)
    console.log(`[METRIC] FCP: ${Math.round((timestamp - navStartTs) * 1000)}ms`);
});
// Attach BEFORE Page.navigate; both timestamps use CDP seconds (same reference frame).
// Do NOT use performance.now() — Node process time has a different origin than CDP timestamps.
// Caveat: 'commit' may fire before listener registration on a reused tab; prefer Approach A.
```


## memory

**Trigger phrases:** "memory leak", "heap", "detached nodes", "retained objects", "growing memory", "GC pressure"

**Domains:** `HeapProfiler.enable`

**Key events/methods:**
- `HeapProfiler.takeHeapSnapshot` → fires `HeapProfiler.addHeapSnapshotChunk` events → parse JSON
- Fields: `snapshot.strings`, `snapshot.nodes` (node_fields: type, name, id, self_size, edge_count, detachedness)
- Detachedness field value `1` = detached DOM node

**Output prefixes:** `[PERFORMANCE]` `[METRIC]` `[FINDING]`

**`[FINDING]` conditions to emit:**
- Any type retains > 5MB → `[FINDING] HIGH_RETENTION: "${name}" ${MB}MB`
- Detached DOM nodes > 50 → `[FINDING] DETACHED_NODES: ${n} detached nodes`
- `(closure)` or `Array` in top-10 retained types with high size → `[FINDING] POSSIBLE_CLOSURE_LEAK`

**Pattern:** Use `Heap Memory Audit` pattern from `SCRIPT_PATTERNS.md`.


## dom

**Trigger phrases:** "DOM", "elements", "structure", "HTML", "rendering issues", "layout", "selectors", "what's on the page"

**Domains:** `DOM.enable`, `Runtime.enable`

**Key events/methods:**
- `DOM.getDocument(depth:2)` → root node tree
- `Runtime.evaluate` → arbitrary DOM queries (`querySelectorAll`, `innerHTML`, `textContent`)

**Output prefixes:** `[DOM]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- Total elements > 1500 → `[FINDING] LARGE_DOM: ${n} elements` *(suggested default — complex apps may legitimately exceed this)*
- `img:not([alt])` count > 0 → `[FINDING] ACCESSIBILITY: ${n} images missing alt`
- `input:not([aria-label]):not([id])` > 0 → `[FINDING] ACCESSIBILITY: ${n} inputs missing label`
- `button:empty` count > 0 → `[FINDING] ACCESSIBILITY: ${n} empty buttons`
- `script:not([src])` count > 0 → `[FINDING] INLINE_SCRIPTS: ${n} inline scripts present`

**Pattern:** Use `DOM + Accessibility Audit` pattern from `SCRIPT_PATTERNS.md`.


## css-coverage

**Trigger phrases:** "unused CSS", "CSS coverage", "dead styles", "remove CSS", "style bloat"

**Domains:** `DOM.enable`, `CSS.enable` (**DOM first**)

**Key events/methods:**
- `CSS.startRuleUsageTracking` → start before navigation
- `CSS.stopRuleUsageTracking` → returns `ruleUsage[]` with `{styleSheetId, used: bool}`
- `CSS.getStyleSheetText(styleSheetId)` → get actual CSS text for unused rules

**Output prefixes:** `[CSS]` `[METRIC]` `[FINDING]`

**`[FINDING]` conditions to emit:**
- Unused rules > 50% of total → `[FINDING] CSS_BLOAT: ${pct}% unused CSS rules` *(adjust % for your app's load pattern)*
- Single stylesheet with > 200 unused rules → `[FINDING] LARGE_UNUSED_SHEET: ${url}`


## js-coverage

**Trigger phrases:** "unused JS", "dead code", "JS coverage", "which functions run", "code not executed"

**Domains:** `Profiler.enable`, `Debugger.enable` (for source map resolution — enabled automatically by `createSourceMapResolver`)

**Key events/methods:**
- `Profiler.startPreciseCoverage(detailed:true, allowTriggeredUpdates:true)` → start before navigation
- `Profiler.takePreciseCoverage` → returns `result[]` → `{url, scriptId, functions[{functionName, ranges[{count, startOffset}]}]}`

**Output prefixes:** `[METRIC]` `[FINDING]` `[SOURCEMAP]`

**`[FINDING]` conditions to emit:**
- Function with `count === 0` in a non-vendor file → `[FINDING] DEAD_CODE: ${displayName} in ${loc} never called`
- Script where > 80% of functions have count = 0 → `[FINDING] DEAD_SCRIPT: ${url} mostly unused`

**Minified bundle caveat — use source map resolver when the bundle is minified:**
Production bundles mangle function names to 1-2 characters (`c`, `i`, `Tt`, `Ut`). The `sourcemap-resolver.mjs` module resolves these back to original names. Use this pre-built script:

```js
export async function run(cdp) {
  // ── Source map resolver (must be first — needs scriptParsed before nav) ───
  const { createSourceMapResolver } = await import(
    new URL('./sourcemap-resolver.mjs', import.meta.url).href
  );
  const resolver = await createSourceMapResolver(cdp);

  // ── Enable coverage ───────────────────────────────────────────────────────
  await cdp.send('Profiler.enable', {});
  await cdp.send('Profiler.startPreciseCoverage', {
    detailed: true,
    allowTriggeredUpdates: true,
  });

  // ── Navigate ──────────────────────────────────────────────────────────────
  await cdp.send('Page.enable', {});
  await cdp.send('Network.enable', {});
  await cdp.send('Page.navigate', { url: TARGET_URL });
  await waitForNetworkIdle(cdp, { idleMs: 2000, timeoutMs: 30000 });

  // Wait for all source maps to finish loading
  await resolver.settle();

  // ── Take coverage ─────────────────────────────────────────────────────────
  const { result } = await cdp.send('Profiler.takePreciseCoverage', {});
  await cdp.send('Profiler.stopPreciseCoverage', {});
  await cdp.send('Profiler.disable', {});

  // ── Helper: byte offset → {line, col} ────────────────────────────────────
  function offsetToLineCol(offset, src) {
    let line = 0, col = 0;
    for (let i = 0; i < offset && i < src.length; i++) {
      if (src[i] === '\n') { line++; col = 0; } else col++;
    }
    return [line, col];
  }

  // ── Analyse ───────────────────────────────────────────────────────────────
  let totalFns = 0, deadFns = 0;

  for (const script of result) {
    const { url, scriptId, functions, source } = script;
    if (!url || url.startsWith('chrome-extension') || url.startsWith('data:')) continue;
    // Skip obviously vendor/CDN scripts
    if (/node_modules|\/vendor\//i.test(url)) continue;

    const srcText = source ?? '';
    let scriptDead = 0;

    for (const fn of functions) {
      totalFns++;
      const isUsed = fn.ranges.some(r => r.count > 0);
      if (isUsed) continue;

      scriptDead++;
      deadFns++;

      // Try source map resolution first
      const startOff = fn.ranges[0]?.startOffset ?? 0;
      const [sl, sc] = offsetToLineCol(startOff, srcText);
      const orig = resolver.resolve(scriptId, sl, sc);

      // Pick the best display name
      const rawName = fn.functionName;
      const isMangled = !rawName || rawName.length <= 2 || /^[A-Z][a-z]$/.test(rawName);
      const displayName = orig?.name ?? (isMangled ? null : rawName);
      if (!displayName) continue; // Skip unresolvable mangled names

      const loc = orig
        ? `${orig.source?.split('/').slice(-2).join('/') ?? 'unknown'}:${orig.line}`
        : url.split('/').pop();
      console.log(`[FINDING] DEAD_CODE: ${displayName} in ${loc}`);
    }

    const total = functions.length;
    if (total > 0 && scriptDead / total > 0.8)
      console.log(`[FINDING] DEAD_SCRIPT: ${url.split('/').pop()} — ${scriptDead}/${total} functions unused`);
  }

  console.log(`[METRIC] JS coverage: ${totalFns} total functions, ${deadFns} unused`);
  resolver.printSummary();
}
```

**Always stop coverage when done** (already included in the pre-built script above):
```js
await cdp.send('Profiler.stopPreciseCoverage', {});
await cdp.send('Profiler.disable', {});
```


## security

**Trigger phrases:** "security", "cookies", "tokens", "auth", "CSP", "headers", "data exfil", "what's being leaked", "credentials", "session", "localStorage", "is this safe"

**Domains:** `Network.enable`, `Runtime.enable`, `DOM.enable`, `Page.enable`

**Key events/methods:**
- `Network.getCookies({ urls: [TARGET_URL] })` → cookies scoped to the target URL only (use this — `getAllCookies` returns the entire browser jar and floods output with third-party ad cookies)
- `Network.requestWillBeSent` + `Network.getRequestPostData(requestId)` → actual POST bodies
- `Network.responseReceived` → inspect response headers: `Content-Security-Policy`, `Strict-Transport-Security`, `X-Frame-Options`, `X-Content-Type-Options`
- `Runtime.evaluate` → `JSON.stringify(Object.entries(localStorage))` → dump localStorage
- `Runtime.evaluate` → `JSON.stringify(Object.entries(sessionStorage))` → dump sessionStorage
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

**Pattern:** See **Security Audit** pre-built pattern in `SCRIPT_PATTERNS.md`.


## websocket

**Trigger phrases:** "websocket", "WS", "socket", "real-time", "socket frames", "what's sent over WS"

**Domains:** `Network.enable`

**Key events/methods:**
- `Network.webSocketCreated` → `{requestId, url}` — WS endpoint established
- `Network.webSocketHandshakeResponseReceived` → headers, status
- `Network.webSocketFrameSent` → `{requestId, timestamp, response: {payloadData}}`
- `Network.webSocketFrameReceived` → `{requestId, timestamp, response: {payloadData}}`
- `Network.webSocketClosed` → `{requestId, timestamp}`

**Output prefixes:** `[NETWORK]` `[FINDING]` `[METRIC]`

**`[FINDING]` conditions to emit:**
- WS endpoint on unknown third-party domain → `[FINDING] WS_UNKNOWN_HOST: ${url}`
- Frame payload contains `token`, `password`, `key` → `[FINDING] SENSITIVE_IN_WS_FRAME: ${snippet}`
- Base64-encoded payload (matches `/^[A-Za-z0-9+/]{40,}={0,2}$/`) → `[FINDING] WS_BASE64_FRAME: possible encoded data`
- Frame size > 100KB → `[FINDING] LARGE_WS_FRAME: ${kb}KB sent` *(threshold is a default — adjust for your protocol)*


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
5. Run synchronous checks: `Performance.getMetrics`, DOM queries, `Network.getCookies({ urls: [TARGET_URL] })` (not `getAllCookies` — that returns the entire browser jar), localStorage dump, `DOMDebugger.getEventListeners`
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


## automate

**Trigger phrases:** "automate", "do X", "click the button", "fill the form", "type into", "submit", "perform this flow", "go through the steps", "interact with", "navigate to X then Y"

**Purpose:** Drive the browser through a multi-step user flow. Use JS evaluation for interactions — it's faster and more reliable than CDP Input events for most cases.

**Domains:** `Page.enable`, `Runtime.enable`, `Network.enable` (add others as needed for observation)

**Interaction toolkit — prefer `Runtime.evaluate` for most actions:**

```js
// Click element by selector
await cdp.send('Runtime.evaluate', { expression: `document.querySelector('${sel}')?.click()` });

// Type into input
await cdp.send('Runtime.evaluate', {
  expression: `
    const el = document.querySelector('${sel}');
    el.focus();
    el.value = ${JSON.stringify(value)};
    el.dispatchEvent(new Event('input', {bubbles:true}));
    el.dispatchEvent(new Event('change', {bubbles:true}));
  `
});

// Wait for element to appear AND be actionable (visible + enabled + pointer-events accessible)
// Returns true when ready; throws on timeout
async function waitFor(cdp, selector, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const { result } = await cdp.send('Runtime.evaluate', {
      expression: `(function() {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) return false;
        const s = window.getComputedStyle(el);
        const r = el.getBoundingClientRect();
        return r.width > 0 && r.height > 0
               && s.display !== 'none' && s.visibility !== 'hidden' && s.opacity !== '0'
               && !el.disabled && !el.closest('[disabled]');
      })()`,
      returnByValue: true,
    });
    if (result.value) return true;
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error(`Timeout waiting for "${selector}" (${timeoutMs}ms)`);
}

// Wait for network idle — event-driven, tracks every request/response pair
// Attach listeners BEFORE navigating so early requests aren't missed
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
      if (pending === 0) idleTimer = setTimeout(() => { clearTimeout(deadline); resolve(); }, idleMs);
    };
    cdp.on('Network.requestWillBeSent',      () => { pending++; clearTimeout(idleTimer); });
    cdp.on('Network.loadingFinished',        () => { pending = Math.max(0, pending - 1); scheduleIdle(); });
    cdp.on('Network.loadingFailed',          () => { pending = Math.max(0, pending - 1); scheduleIdle(); });
    cdp.on('Network.requestServedFromCache', () => { pending = Math.max(0, pending - 1); scheduleIdle(); });
    scheduleIdle(); // resolve immediately if already idle
  });
}

// Scroll to bottom
await cdp.send('Runtime.evaluate', { expression: `window.scrollTo(0, document.body.scrollHeight)` });

// Submit form
await cdp.send('Runtime.evaluate', { expression: `document.querySelector('${formSel}')?.submit()` });

// Navigate then wait for idle
const idlePromise = waitForNetworkIdle(cdp, { idleMs: 500, timeoutMs: 15000 }); // attach BEFORE navigate
await cdp.send('Page.navigate', { url });
await idlePromise;

// OR use Page.lifecycleEvent (simpler, less precise)
await cdp.send('Page.setLifecycleEventsEnabled', { enabled: true });
// then wait for 'Page.lifecycleEvent' with name 'networkIdle' or 'load'
```

Canonical copy-paste versions of `waitForNetworkIdle` and `waitForSelector` with full options → `SCRIPT_PATTERNS.md`.

**Output prefixes:** `[AUTOMATE]` `[FINDING]` `[ACTION]` `[METRIC]`

**`[AUTOMATE]` prefix:** Emit for each step executed: `[AUTOMATE] clicked "${selector}"`, `[AUTOMATE] typed into "${selector}"`, `[AUTOMATE] waited for "${selector}" — found after Nms`

**`[FINDING]` conditions:**
- `waitFor` times out → `[FINDING] ELEMENT_NOT_FOUND: "${selector}" not present after ${timeout}ms`
- Navigation results in error page → `[FINDING] NAVIGATION_FAILED: ${url}`
- Step throws exception → `[FINDING] STEP_FAILED: ${step} — ${error}`

**Step pattern:**

```js
export async function run(cdp) {
  await cdp.send('Page.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Network.enable', {});

  // paste waitFor helper here

  // Step 1: navigate
  await cdp.send('Page.navigate', { url: 'https://example.com' });
  await waitFor(cdp, 'body');
  console.log('[AUTOMATE] navigated to https://example.com');

  // Step 2: click
  await waitFor(cdp, '#submit-btn');
  await cdp.send('Runtime.evaluate', { expression: `document.querySelector('#submit-btn').click()` });
  console.log('[AUTOMATE] clicked #submit-btn');

  // Step 3: verify outcome
  await waitFor(cdp, '.success-message');
  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `document.querySelector('.success-message')?.textContent`, returnByValue: true,
  });
  console.log(`[AUTOMATE] success message: "${result.value}"`);
  console.log('[METRIC] Flow completed successfully');
}
```

**Combine with:** `debug` to catch errors mid-flow, `network` to observe API calls during the flow, `screenshot` to capture state after each step.


## login

**Trigger phrases:** "login", "sign in", "log me in", "authenticate", "enter credentials", "fill the login form", "auth flow"

**Purpose:** Automate an authentication flow — navigate to login page, fill credentials, submit, verify auth success, capture session state. Combine with `security` to audit what tokens/cookies are set post-login.

**Domains:** `Page.enable`, `Runtime.enable`, `Network.enable`

**Key steps:**
1. Navigate to login URL
2. Wait for username/password fields
3. Fill credentials via `Runtime.evaluate` with real `input` + `change` events
4. Click submit or press Enter
5. Wait for redirect / dashboard element
6. Verify logged-in state
7. Optionally dump cookies and localStorage (combine with `security`)

**Selector discovery — when selectors are unknown:**
```js
// Find likely username field
const { result } = await cdp.send('Runtime.evaluate', {
  expression: `document.querySelector('input[type="email"], input[type="text"], input[name*="user"], input[name*="email"], input[id*="user"], input[id*="email"]')?.name`,
  returnByValue: true,
});

// Find likely password field
// input[type="password"] — usually the most reliable password selector
```

**Credential injection pattern (triggers React/Vue/framework state):**
```js
async function fillField(cdp, selector, value) {
  await cdp.send('Runtime.evaluate', {
    expression: `
      const el = document.querySelector('${selector}');
      const nativeInputValueSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
      nativeInputValueSetter.call(el, ${JSON.stringify(value)});
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    `
  });
  console.log('[AUTOMATE] filled ${selector}');
}
```

**Post-login verification:**
```js
// Check URL changed (redirect after login)
const { result: url } = await cdp.send('Runtime.evaluate', { expression: 'location.href', returnByValue: true });
console.log(`[AUTOMATE] post-login URL: ${url.value}`);

// Check for auth cookie
const { cookies } = await cdp.send('Network.getAllCookies', {});
const authCookie = cookies.find(c => /session|token|auth/i.test(c.name));
if (authCookie) console.log(`[AUTOMATE] auth cookie set: ${authCookie.name}`);
else console.log('[FINDING] NO_AUTH_COOKIE: no session/token cookie after login');

// Check localStorage for token
const { result: token } = await cdp.send('Runtime.evaluate', {
  expression: `localStorage.getItem('token') || localStorage.getItem('authToken') || localStorage.getItem('jwt')`,
  returnByValue: true,
});
if (token.value) console.log(`[AUTOMATE] token in localStorage: ${token.value.slice(0, 20)}...`);
```

**`[FINDING]` conditions:**
- Login form not found → `[FINDING] LOGIN_FORM_NOT_FOUND: no username/password fields on ${url}`
- No redirect after submit → `[FINDING] LOGIN_NO_REDIRECT: URL unchanged after submit`
- Error message visible → `[FINDING] LOGIN_FAILED: error element present — "${errorText}"`
- No auth cookie and no token in storage → `[FINDING] NO_SESSION: login appeared to succeed but no session established`
- Auth request returned non-200 → `[FINDING] AUTH_HTTP_ERROR: ${status} on ${url}`

**Combine with:** `security` (audit tokens post-login), `network` (capture auth API calls), `debug` (catch login errors).


## user-auth

**Trigger phrases:** "let me log in myself", "I'll authenticate", "manual login", "open browser so I can sign in", "auth flow", "open visible browser", "I need to authenticate first", "let me sign in then scrape", "open browser non-headless for auth"

**Purpose:** Opens Chrome in **visible** mode (never headless), navigates to the login URL, then polls passively for authentication completion while the user completes any auth flow (password, 2FA, SSO, CAPTCHA, OAuth). Once auth is detected the browser **stays open** so all subsequent CDP scripts reuse the authenticated session.

**Key distinction from `login`:**
- `login` = agent injects credentials programmatically via CDP
- `user-auth` = user completes auth manually; agent waits and detects completion

**Key requirement:** Browser must be visible for the user to interact — do not pass `--headless` to `open-browser.mjs` for this intent.

**Domains required:** `Network`, `Page`, `Runtime`

**Critical flags:**
- `open-browser.mjs` → omit `--headless` (default is already non-headless; just don't add it)
- `cdp-sandbox.mjs` → add `--keep-tab` so the tab and session stay open after the script exits
- Do NOT call `cleanup()` in `run()` — the browser must stay alive for subsequent tasks


**Pre-built user-auth script:**

```js
// Polls for auth completion — user authenticates manually in the visible browser.
// Emits [AUTH_COMPLETE] when detected or [AUTH_TIMEOUT] after TIMEOUT_MS.
// Browser must be opened WITHOUT --headless.

export async function run(cdp) {
  // ── CONFIGURE ────────────────────────────────────────────────────────────
  const LOGIN_URL          = 'https://example.com/login'; // ← required
  const POST_AUTH_PATTERN  = '/dashboard';                // ← URL fragment that appears after login
  const AUTH_COOKIE_NAME   = '';                          // ← optional: exact cookie name to wait for
  const TIMEOUT_MS         = 120_000;                     // ← 2 min; increase for slow SSO / 2FA flows
  const POLL_MS            = 2_000;
  // ─────────────────────────────────────────────────────────────────────────

  const { writeFileSync } = await import('fs');
  const { join }          = await import('path');

  await cdp.send('Network.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Runtime.enable', {});

  // Navigate to login page
  await cdp.send('Page.navigate', { url: LOGIN_URL });
  await new Promise(r => setTimeout(r, 1500)); // let page settle

  // ── Helper: detect auth from current page state ────────────────────────
  async function checkAuthState() {
    const { result: urlRes } = await cdp.send('Runtime.evaluate', {
      expression: 'location.href', returnByValue: true,
    });
    const currentUrl = urlRes.value ?? '';

    const { result: storageRes } = await cdp.send('Runtime.evaluate', {
      expression: `
        JSON.stringify({
          local:   localStorage.getItem('token') || localStorage.getItem('authToken') ||
                   localStorage.getItem('jwt')   || localStorage.getItem('access_token') ||
                   localStorage.getItem('id_token') || null,
          session: sessionStorage.getItem('token') || sessionStorage.getItem('authToken') ||
                   sessionStorage.getItem('jwt')   || null,
        })
      `,
      returnByValue: true,
    });
    let storage = { local: null, session: null };
    try { storage = JSON.parse(storageRes.value ?? '{}'); } catch { /* ignore */ }

    const { cookies } = await cdp.send('Network.getAllCookies', {});
    const authCookies = cookies.filter(c => /session|token|auth|jwt|sid/i.test(c.name));

    const byUrl     = POST_AUTH_PATTERN && currentUrl.includes(POST_AUTH_PATTERN);
    const byCookie  = AUTH_COOKIE_NAME
      ? cookies.some(c => c.name === AUTH_COOKIE_NAME)
      : authCookies.length > 0;
    const byStorage = !!(storage.local || storage.session);

    return {
      detected: byUrl || byCookie || byStorage,
      method:   byUrl ? 'url-pattern' : byCookie ? 'auth-cookie' : byStorage ? 'storage-token' : null,
      currentUrl,
      authCookies,
      storage,
    };
  }

  // ── Check if already authenticated before waiting ──────────────────────
  const preCheck = await checkAuthState();
  if (preCheck.detected) {
    console.log(`[AUTH] Already authenticated via ${preCheck.method} — skipping wait`);
    console.log(`[AUTH_COMPLETE] url=${preCheck.currentUrl}`);
    writeFileSync(join(cdp.outputDir, 'auth-state.json'), JSON.stringify({
      alreadyAuthenticated: true,
      ...preCheck,
      detectedAt: new Date().toISOString(),
    }, null, 2));
    console.log(`[FINDING] auth-state.json → ${join(cdp.outputDir, 'auth-state.json')}`);
    return;
  }

  // ── Poll loop ──────────────────────────────────────────────────────────
  console.log(`[AUTH] Browser is open — please complete authentication in the browser window`);
  console.log(`[AUTH] Waiting up to ${TIMEOUT_MS / 1000}s for auth signals…`);

  const start       = Date.now();
  let authDetected  = false;

  while (Date.now() - start < TIMEOUT_MS) {
    const elapsed = ((Date.now() - start) / 1000).toFixed(0);
    const state   = await checkAuthState();

    if (state.detected) {
      authDetected = true;

      const authState = {
        detectedAt:   new Date().toISOString(),
        elapsedSec:   Number(elapsed),
        method:       state.method,
        url:          state.currentUrl,
        authCookies:  state.authCookies.map(c => ({
          name: c.name, domain: c.domain, httpOnly: c.httpOnly, secure: c.secure,
        })),
        hasLocalStorageToken:  !!state.storage.local,
        hasSessionStorageToken: !!state.storage.session,
      };

      const authPath = join(cdp.outputDir, 'auth-state.json');
      writeFileSync(authPath, JSON.stringify(authState, null, 2));

      console.log(`[AUTH_COMPLETE] authenticated after ${elapsed}s via ${state.method}`);
      console.log(`[AUTH] url=${state.currentUrl}`);
      console.log(`[AUTH] auth cookies=${state.authCookies.map(c => c.name).join(', ') || 'none'}`);
      console.log(`[AUTH] localStorage token=${!!state.storage.local}, sessionStorage token=${!!state.storage.session}`);
      console.log(`[FINDING] auth-state.json → ${authPath}`);
      console.log(`[ACTION] Run subsequent CDP scripts against the same Chrome port — session is authenticated`);
      break;
    }

    console.log(`[AUTH] [${elapsed}s/${TIMEOUT_MS / 1000}s] waiting… url=${state.currentUrl.slice(-60)}`);
    await new Promise(r => setTimeout(r, POLL_MS));
  }

  if (!authDetected) {
    console.log(`[AUTH_TIMEOUT] user did not authenticate within ${TIMEOUT_MS / 1000}s`);
    console.log(`[ACTION] Increase TIMEOUT_MS, verify POST_AUTH_PATTERN matches the post-login URL, or check AUTH_COOKIE_NAME`);
  }

  // ── Do NOT call cleanup() — browser must stay open for subsequent scripts ──
}
```


**Agent loop contract:**

```
REASON   → need authenticated session before running scrape / debug / security tasks
OPEN     → open-browser.mjs --profile Default --port 9222   ← NO --headless
WAIT     → run user-auth script with --keep-tab
           watch stdout for [AUTH_COMPLETE] or [AUTH_TIMEOUT]
CONTINUE → once [AUTH_COMPLETE], run subsequent scripts on the same port/session
REUSE    → Chrome stays open; subsequent scripts connect without re-authenticating
STOP WHEN: [AUTH_COMPLETE] emitted (or [AUTH_TIMEOUT] — handle appropriately)
```

**Example shell sequence:**
```bash
SKILL_DIR=<path-to-skill>
TMPDIR=$(node -e "process.stdout.write(require('os').tmpdir())")

# 1. Open Chrome visibly. If output contains "isolated": true, log in in that CDP window.
node "$SKILL_DIR/scripts/open-browser.mjs" --profile Default --port 9222

# 2. Run user-auth: agent waits while user logs in
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-user-auth.mjs" \
  --new-tab "about:blank" --keep-tab \
  > "$TMPDIR/cdp-auth-output.txt" 2>&1
# → tail "$TMPDIR/cdp-auth-output.txt" and wait for [AUTH_COMPLETE]

# 3. Run subsequent scripts on the same authenticated session
node "$SKILL_DIR/scripts/cdp-sandbox.mjs" "$TMPDIR/cdp-scrape.mjs" \
  > "$TMPDIR/cdp-scrape-output.txt" 2>&1

# 4. Do NOT close Chrome — leave it running for further tasks
```

**Combine with:** `scrape` (extract authenticated data), `security` (audit session tokens after login), `debug` (investigate auth-gated behavior), `network` (capture post-auth API calls).

**Auth detection signals (in precedence order):**
1. URL changes to `POST_AUTH_PATTERN` (most reliable — redirect after login)
2. Auth cookie set (name matches `/session|token|auth|jwt|sid/i` or `AUTH_COOKIE_NAME`)
3. `localStorage` / `sessionStorage` token key present

**Tuning guide:**
| Situation | Fix |
|---|---|
| `[AUTH_TIMEOUT]` — login redirects to subdomain | change `POST_AUTH_PATTERN` to a path fragment, not full URL |
| `[AUTH_TIMEOUT]` — SSO/SAML takes longer than 2 min | increase `TIMEOUT_MS` to `300_000` |
| False positive detection | set `AUTH_COOKIE_NAME` to the exact cookie set on your app |
| Already logged in in the CDP-controlled session | script auto-detects and emits `[AUTH_COMPLETE]` immediately |


## scrape

**Trigger phrases:** "scrape", "extract", "collect data", "pull content", "harvest", "get all X from the page", "list all Y", "export data"

**Purpose:** Extract structured data from the live DOM. Faster and more reliable than string parsing — queries the actual rendered state including JS-rendered content.

**Domains:** `Runtime.enable`, `DOM.enable`, `Page.enable` (if navigation needed)

**Pre-built scrape script:**

```js
export async function run(cdp) {
  const TARGET_URL = 'https://example.com'; // ← set target URL

  await cdp.send('Runtime.enable', {});
  await cdp.send('DOM.enable', {});
  await cdp.send('Page.enable', {});
  await cdp.send('Network.enable', {});

  const { writeFileSync } = await import('fs');
  const { join } = await import('path');

  // Dialog guard
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: true }));

  // Navigate and wait for load
  let loadFired = false;
  cdp.on('Page.loadEventFired', () => { loadFired = true; });
  await cdp.send('Page.navigate', { url: TARGET_URL });
  const deadline = Date.now() + 15000;
  while (!loadFired && Date.now() < deadline) await new Promise(r => setTimeout(r, 200));
  await new Promise(r => setTimeout(r, 500)); // settle JS-rendered content

  // ── Verify page state ─────────────────────────────────────────────────────
  const { result: readyState } = await cdp.send('Runtime.evaluate', {
    expression: 'document.readyState', returnByValue: true });
  const { result: titleRes } = await cdp.send('Runtime.evaluate', {
    expression: 'document.title', returnByValue: true });
  console.log(`[METRIC] URL: ${TARGET_URL} | readyState: ${readyState.value} | title: "${titleRes.value}"`);

  if (readyState.value !== 'complete') {
    console.log(`[FINDING] SCRAPE_NOT_READY: readyState=${readyState.value} — add longer wait or use waitForNetworkIdle`);
    return;
  }

  // ── Detect consent / GDPR wall ────────────────────────────────────────────
  // Symptoms: page title in non-English, tiny request count, no content
  const { result: consentWall } = await cdp.send('Runtime.evaluate', {
    expression: `(function() {
      const btns = [...document.querySelectorAll('button,a')];
      const acceptBtn = btns.find(b => /accept|agree|לקבל|הכול|accepter|akzept|aceptar/i.test(b.innerText || b.textContent || ''));
      const isConsentPage = !!(
        document.querySelector('[class*="consent"],[id*="consent"],[class*="gdpr"],[id*="guce"]') ||
        (acceptBtn && document.querySelectorAll('table,main article,[data-symbol]').length === 0)
      );
      return JSON.stringify({ isConsentPage, hasAcceptBtn: !!acceptBtn, title: document.title });
    })()`,
    returnByValue: true,
  });
  const consentInfo = JSON.parse(consentWall.value);
  if (consentInfo.isConsentPage) {
    if (consentInfo.hasAcceptBtn) {
      console.log('[FINDING] SCRAPE_CONSENT_WALL: GDPR/consent dialog detected — accepting and re-navigating');
      await cdp.send('Runtime.evaluate', {
        expression: `([...document.querySelectorAll('button,a')].find(b => /accept|agree|לקבל|הכול|accepter|akzept|aceptar/i.test(b.innerText||b.textContent||''))||{click:()=>{}}).click()`
      });
      await new Promise(r => setTimeout(r, 1500));
      let renavLoaded = false;
      cdp.on('Page.loadEventFired', () => { renavLoaded = true; });
      await cdp.send('Page.navigate', { url: TARGET_URL });
      const renavDeadline = Date.now() + 15000;
      while (!renavLoaded && Date.now() < renavDeadline) await new Promise(r => setTimeout(r, 200));
      await new Promise(r => setTimeout(r, 1000));
    } else {
      console.log('[FINDING] SCRAPE_CONSENT_WALL: consent dialog detected but no accept button found — manual intervention needed');
      return;
    }
  }

  // ── Detect auth wall ──────────────────────────────────────────────────────
  const { result: authWall } = await cdp.send('Runtime.evaluate', {
    expression: `!!(document.querySelector('input[type="password"]') ||
      document.querySelector('[class*="login"],[id*="login"],[class*="signin"],[id*="signin"]'))`,
    returnByValue: true,
  });
  if (authWall.value) {
    console.log('[FINDING] SCRAPE_REQUIRES_AUTH: login wall detected — run login intent first');
    return;
  }

  const results = { url: TARGET_URL, title: titleRes.value, scrapedAt: new Date().toISOString() };

  // Selector priority: data-* attrs → semantic HTML (article, li) → aria/role → text+link fallback.
  // Avoid [class*="..."] — CSS-module class names are hashed and change on every build.

  // ── Links ─────────────────────────────────────────────────────────────────
  const { result: linksRes } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify([...document.querySelectorAll('a[href]')]
      .map(a => ({ text: a.innerText.trim().slice(0, 120), href: a.href }))
      .filter(a => a.text && !a.href.startsWith('javascript:')))`,
    returnByValue: true,
  });
  results.links = JSON.parse(linksRes.value ?? '[]');
  console.log(`[SCRAPE] Links: ${results.links.length}`);
  if (results.links.length === 0)
    console.log('[FINDING] SCRAPE_EMPTY: no links — page may be JS-rendered or require auth');
  results.links.slice(0, 10).forEach(l => console.log(`[SCRAPE] link: "${l.text}" → ${l.href}`));

  // ── Tables ────────────────────────────────────────────────────────────────
  const { result: tableRes } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify([...document.querySelectorAll('table')].map(t => ({
      headers: [...t.querySelectorAll('th')].map(th => th.innerText.trim()),
      rows:    [...t.querySelectorAll('tbody tr')].map(tr =>
                 [...tr.querySelectorAll('td')].map(td => td.innerText.trim())),
    })))`,
    returnByValue: true,
  });
  results.tables = JSON.parse(tableRes.value ?? '[]');
  if (results.tables.length > 0)
    console.log(`[SCRAPE] Tables: ${results.tables.length} (rows: ${results.tables.map(t => t.rows.length).join(', ')})`);

  // ── Lists ─────────────────────────────────────────────────────────────────
  const { result: listRes } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify([...document.querySelectorAll('ul li, ol li')]
      .map(li => li.innerText.trim()).filter(Boolean).slice(0, 200))`,
    returnByValue: true,
  });
  results.listItems = JSON.parse(listRes.value ?? '[]');
  if (results.listItems.length > 0)
    console.log(`[SCRAPE] List items: ${results.listItems.length}`);

  // ── Meta / OpenGraph ──────────────────────────────────────────────────────
  const { result: metaRes } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify([...document.querySelectorAll('meta[name],meta[property]')]
      .map(m => ({ key: m.name || m.getAttribute('property'), value: m.content }))
      .filter(m => m.key && m.value))`,
    returnByValue: true,
  });
  results.meta = JSON.parse(metaRes.value ?? '[]');

  // ── Pagination signal ─────────────────────────────────────────────────────
  const { result: hasNext } = await cdp.send('Runtime.evaluate', {
    expression: `!!(document.querySelector('[class*="next"]:not([disabled]),[aria-label*="Next"],[rel="next"]'))`,
    returnByValue: true,
  });
  if (hasNext.value)
    console.log('[METRIC] PAGINATION_DETECTED: next page exists — use multi-page loop to continue');

  // ── Results summary ───────────────────────────────────────────────────────
  const total = results.links.length + results.listItems.length +
                results.tables.reduce((s, t) => s + t.rows.length, 0);
  console.log(`\n[SCRAPE] === RESULTS ===`);
  console.log(`[SCRAPE] Page: "${titleRes.value}"`);
  console.log(`[SCRAPE] Links: ${results.links.length}  Tables: ${results.tables.length}  List items: ${results.listItems.length}  Total: ${total}`);
  if (total === 0)
    console.log('[FINDING] SCRAPE_EMPTY: zero data points extracted — check selectors or auth state');

  // ── Save to outputDir ─────────────────────────────────────────────────────
  const outputPath = join(cdp.outputDir, 'scrape-results.json');
  writeFileSync(outputPath, JSON.stringify(results, null, 2));
  console.log(`[FINDING] scrape-results.json → ${outputPath}`);
}
```

**Multi-page loop (add inside the run() after extracting one page):**
```js
while (hasNext.value) {
  await cdp.send('Runtime.evaluate', {
    expression: `document.querySelector('[class*="next"]:not([disabled]),[aria-label*="Next"]')?.click()` });
  await new Promise(r => setTimeout(r, 1500)); // wait for JS render
  // re-run extraction block and push to results array
  const { result: nextCheck } = await cdp.send('Runtime.evaluate', {
    expression: `!!(document.querySelector('[class*="next"]:not([disabled]),[aria-label*="Next"]'))`,
    returnByValue: true });
  if (!nextCheck.value) break;
}
```

**Output prefixes:** `[SCRAPE]` `[METRIC]` `[FINDING]`

**`[FINDING]` conditions:**
- Zero items extracted → `[FINDING] SCRAPE_EMPTY: selector returned 0 elements — page may require interaction or login`
- Page requires login (login form detected) → `[FINDING] SCRAPE_REQUIRES_AUTH: login wall present`
- GDPR/consent dialog intercepted navigation → `[FINDING] SCRAPE_CONSENT_WALL: consent dialog detected — accepted and re-navigating` (auto-handled by pre-built script; see `RECOVERY.md` for symptoms and manual fix)
- Data appears JS-rendered but `readyState` check fails → `[FINDING] SCRAPE_NOT_READY: content may not be loaded yet`
- Pagination detected → `[METRIC] PAGINATION_DETECTED: next page exists — use multi-page loop`

**Agent loop usage:**

```
REASON  → what data do I need? which page/selector?
SCRAPE  → set TARGET_URL, run script → read [SCRAPE] + [FINDING] lines
SAVE    → results auto-saved to cdp.outputDir/scrape-results.json
PAGINATE → if [METRIC] PAGINATION_DETECTED → add multi-page loop and re-run
AUTH    → if [FINDING] SCRAPE_REQUIRES_AUTH → run login intent first, then re-run
```

Stop when: `[SCRAPE] Total: N` is non-zero and no `[FINDING] SCRAPE_EMPTY` and no more pagination.

**Combine with:** `login` (authenticate first), `emulate` (scrape as mobile), `screenshot` (capture visual proof).


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


**Pre-built emulate script:**

```js
// Full mobile emulation — viewport + DPR + touch + UA + Sec-CH-UA hints
// Call setDeviceMetricsOverride + setTouchEmulationEnabled BEFORE Page.navigate

export async function run(cdp) {
  const TARGET_URL = 'https://example.com'; // ← set URL

  // ── Pick a device preset ──────────────────────────────────────────────────
  const DEVICE = {
    name:           'Pixel 7',
    width:          412,
    height:         915,
    deviceScaleFactor: 2.625,
    mobile:         true,
    userAgent:      'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/<current-version>.0.0.0 Mobile Safari/537.36',
    // Sec-CH-UA hints — sent in request headers when provided
    userAgentMetadata: {
      brands: [
        { brand: 'Chromium',       version: '<current-major>' },
        { brand: 'Google Chrome',  version: '<current-major>' },
        { brand: 'Not=A?Brand',    version: '24'  },
      ],
      fullVersion:    '<current-version>.0.0.0',
      platform:       'Android',
      platformVersion: '14',
      architecture:   '',
      model:          'Pixel 7',
      mobile:         true,
    },
  };

  await cdp.send('Network.enable', {});
  await cdp.send('Page.enable', {});

  // 1. Set viewport + DPR + mobile flag
  await cdp.send('Emulation.setDeviceMetricsOverride', {
    width:             DEVICE.width,
    height:            DEVICE.height,
    deviceScaleFactor: DEVICE.deviceScaleFactor,
    mobile:            DEVICE.mobile,
  });

  // 2. Enable real touch events (not just pointer events mapped to mouse)
  await cdp.send('Emulation.setTouchEmulationEnabled', {
    enabled:       true,
    maxTouchPoints: 5,
  });

  // 3. Override UA + Sec-CH-UA hints (affects both navigator.userAgent and request headers)
  await cdp.send('Emulation.setUserAgentOverride', {
    userAgent:         DEVICE.userAgent,
    acceptLanguage:    'en-US,en;q=0.9',
    platform:          'Linux armv8l',
    userAgentMetadata: DEVICE.userAgentMetadata,
  });

  console.log(`[EMULATE] device=${DEVICE.name} viewport=${DEVICE.width}x${DEVICE.height} dpr=${DEVICE.deviceScaleFactor} touch=enabled`);

  // 4. Navigate AFTER all overrides are set
  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise(r => setTimeout(r, 3000));

  // 5. Verify emulation is active
  const { result: innerW } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({ innerWidth: window.innerWidth, innerHeight: window.innerHeight, dpr: window.devicePixelRatio, ua: navigator.userAgent.slice(0,60) })`,
    returnByValue: true,
  });
  const metrics = JSON.parse(innerW.value ?? '{}');
  console.log(`[METRIC] innerWidth=${metrics.innerWidth} innerHeight=${metrics.innerHeight} dpr=${metrics.dpr}`);
  console.log(`[METRIC] UA in page: ${metrics.ua}…`);

  // 6. Layout break detection
  const { result: overflow } = await cdp.send('Runtime.evaluate', {
    expression: `document.documentElement.scrollWidth > ${DEVICE.width}`,
    returnByValue: true,
  });
  if (overflow.value)
    console.log(`[FINDING] LAYOUT_BREAK: horizontal scroll detected at ${DEVICE.width}px viewport`);
  else
    console.log(`[EMULATE] Layout: no horizontal overflow`);

  // 7. Restore desktop (if this is a one-off check — omit if combining with screenshot/scrape)
  // await cdp.send('Emulation.clearDeviceMetricsOverride', {});
  // await cdp.send('Emulation.setTouchEmulationEnabled', { enabled: false });
}
```


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

**Pre-built monitor script:**

```js
export async function run(cdp) {
  const TARGET_URL  = '';        // ← set URL to navigate, or '' to monitor the current tab
  const INTERVAL_MS = 5000;      // poll every N ms
  const DURATION_MS = 60000;     // total window (ms) — increase for longer watches

  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Log.enable', {});
  await cdp.send('Page.enable', {});

  const { writeFileSync } = await import('fs');
  const { join } = await import('path');

  // Dialog guard (prevents alert() from blocking evaluate calls)
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: true }));

  // Async error queues — drained each iteration
  const errors  = [];
  const network = [];
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    errors.push(exceptionDetails.exception?.description ?? exceptionDetails.text);
  });
  cdp.on('Network.responseReceived', ({ response }) => {
    if (response.status >= 400) network.push(`HTTP ${response.status} ${response.url}`);
  });
  cdp.on('Log.entryAdded', ({ entry }) => {
    if (entry.level === 'error') errors.push(`[${entry.source}] ${entry.text}`);
  });

  // Optional navigation
  if (TARGET_URL) {
    await cdp.send('Page.navigate', { url: TARGET_URL });
    await new Promise(r => setTimeout(r, 1500));
    console.log(`[MONITOR] navigated to ${TARGET_URL}`);
  }

  const changeLog = [];
  const start = Date.now();
  let iteration = 0;
  let prevState = null;
  let totalErrors = 0;

  console.log(`[MONITOR] === START — watching for ${DURATION_MS / 1000}s, polling every ${INTERVAL_MS / 1000}s ===`);

  while (Date.now() - start < DURATION_MS) {
    iteration++;
    const elapsed = ((Date.now() - start) / 1000).toFixed(0);

    // Snapshot page state
    let current;
    try {
      const { result: snap } = await cdp.send('Runtime.evaluate', {
        expression: `JSON.stringify({
          url:      location.href,
          title:    document.title,
          errorEls: document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length,
          text:     document.body?.innerText?.slice(0, 200) ?? '',
        })`,
        returnByValue: true,
      });
      current = JSON.parse(snap.value ?? '{}');
    } catch {
      console.log(`[FINDING] MONITOR_HANG: evaluate timed out at ${elapsed}s — page may be unresponsive`);
      break;
    }

    // Emit on state change
    if (JSON.stringify(current) !== JSON.stringify(prevState)) {
      console.log(`[MONITOR] [${elapsed}s] state changed: ${JSON.stringify(current)}`);
      changeLog.push({ elapsed: Number(elapsed), ...current });
      if (prevState && current.url !== prevState.url)
        console.log(`[FINDING] MONITOR_REDIRECT: URL changed to ${current.url} at ${elapsed}s`);
      if (prevState && current.errorEls > 0 && current.errorEls > (prevState.errorEls ?? 0))
        console.log(`[FINDING] MONITOR_DOM_ERROR: ${current.errorEls} error elements appeared at ${elapsed}s`);
      prevState = current;
    }

    // Drain error queues
    while (errors.length) {
      const e = errors.shift(); totalErrors++;
      console.log(`[FINDING] MONITOR_ERROR [${elapsed}s]: ${e}`);
    }
    while (network.length) {
      const n = network.shift(); totalErrors++;
      console.log(`[FINDING] MONITOR_NETWORK_ERROR [${elapsed}s]: ${n}`);
    }

    console.log(`[METRIC] [${elapsed}s] iteration ${iteration} | url: ${current.url?.slice(0, 60)}`);
    await new Promise(r => setTimeout(r, INTERVAL_MS));
  }

  // ── OBSERVE summary ──────────────────────────────────────────────────────
  const elapsed = ((Date.now() - start) / 1000).toFixed(0);
  console.log(`\n[MONITOR] === OBSERVE ===`);
  console.log(`[MONITOR] Ran ${iteration} iterations over ${elapsed}s`);
  console.log(`[MONITOR] State changes: ${changeLog.length}  Total errors: ${totalErrors}`);
  if (changeLog.length > 0)
    console.log(`[MONITOR] Changes: ${changeLog.map(c => `${c.elapsed}s→${c.url?.slice(-40)}`).join(', ')}`);

  // ── ACT block ────────────────────────────────────────────────────────────
  console.log(`\n[MONITOR] === ACT ===`);
  if (totalErrors > 0)
    console.log(`[ACTION] ${totalErrors} errors detected — run debug intent on the affected URL for root cause`);
  if (changeLog.some(c => c.errorEls > 0))
    console.log(`[ACTION] DOM error elements appeared — inspect with debug intent`);
  if (totalErrors === 0 && changeLog.length === 0)
    console.log(`[ACTION] No changes or errors in ${elapsed}s — page is stable`);

  // ── Save change log ───────────────────────────────────────────────────────
  const logPath = join(cdp.outputDir, 'monitor-log.json');
  writeFileSync(logPath, JSON.stringify({ iterations: iteration, totalErrors, changeLog }, null, 2));
  console.log(`[FINDING] monitor-log.json → ${logPath}`);
}
```

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


## storage

**Trigger phrases:** "storage", "localStorage", "sessionStorage", "IndexedDB", "cookies", "cache storage", "service worker cache", "check what's stored", "what data does this site store", "storage quota", "offline data", "browser storage"

**Purpose:** Full inventory of all client-side storage — cookies (first and third party), localStorage, sessionStorage, IndexedDB databases and their records, Cache Storage caches, Service Worker registrations, and storage quota. Use for privacy audits, debugging persistence bugs, and session forensics.

**Domains:** `Network.enable`, `Runtime.enable`, `Page.enable`

**Key events/methods:**
- `Network.getAllCookies` → all cookies across all domains in the browser jar (richer than `getCookies`)
- `Runtime.evaluate` → localStorage, sessionStorage, indexedDB.databases(), caches.keys(), navigator.serviceWorker.getRegistrations(), navigator.storage.estimate()
- `Network.getCookies({ urls })` → scoped to a specific origin when full jar is too noisy

**Script skeleton:**

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {}));

  await cdp.send('Page.navigate', { url: 'https://TARGET_URL/' });
  await new Promise(r => setTimeout(r, 5000)); // settle

  // ── Cookies (all domains) ─────────────────────────────────────────────────
  const { cookies } = await cdp.send('Network.getAllCookies', {});
  console.log(`[SECURITY] Cookies total: ${cookies.length}`);
  for (const c of cookies) {
    const flags = [c.httpOnly ? 'httpOnly' : 'NO_httpOnly', c.secure ? 'secure' : 'NO_secure', `sameSite=${c.sameSite || 'None'}`].join(' ');
    console.log(`[SECURITY] Cookie: ${c.name.padEnd(28)} domain=${c.domain} [${flags}]`);
    if (!c.httpOnly) console.log(`[FINDING] COOKIE_NO_HTTPONLY: ${c.name}`);
    if (!c.secure)   console.log(`[FINDING] COOKIE_NO_SECURE: ${c.name}`);
  }

  // ── localStorage ─────────────────────────────────────────────────────────
  const { result: lsR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.entries(localStorage).map(([k,v])=>({k,v:v.length>200?v.slice(0,200)+'...':v,size:new Blob([k+v]).size})))`,
    returnByValue: true,
  });
  const ls = JSON.parse(lsR.value || '[]');
  console.log(`[STORAGE] localStorage: ${ls.length} keys, ~${ls.reduce((s,i)=>s+i.size,0)}B`);
  for (const { k, v } of ls) {
    console.log(`[STORAGE] LS[${k}] = ${v}`);
    if (/token|auth|jwt|secret|password|credential/i.test(k)) console.log(`[FINDING] SENSITIVE_IN_STORAGE: localStorage["${k}"]`);
  }

  // ── sessionStorage ────────────────────────────────────────────────────────
  const { result: ssR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify(Object.entries(sessionStorage).map(([k,v])=>({k,v:v.slice(0,200)})))`,
    returnByValue: true,
  });
  const ss = JSON.parse(ssR.value || '[]');
  console.log(`[STORAGE] sessionStorage: ${ss.length} keys`);
  for (const { k, v } of ss) console.log(`[STORAGE] SS[${k}] = ${v}`);

  // ── IndexedDB ─────────────────────────────────────────────────────────────
  const { result: idbR } = await cdp.send('Runtime.evaluate', {
    expression: `indexedDB.databases().then(dbs=>JSON.stringify(dbs))`,
    returnByValue: true, awaitPromise: true,
  });
  const dbs = JSON.parse(idbR.value || '[]');
  console.log(`[STORAGE] IndexedDB: ${dbs.length} databases`);
  for (const db of dbs) console.log(`[STORAGE] IDB: ${db.name} v${db.version}`);

  // ── Service Workers ───────────────────────────────────────────────────────
  const { result: swR } = await cdp.send('Runtime.evaluate', {
    expression: `navigator.serviceWorker.getRegistrations().then(r=>JSON.stringify(r.map(s=>({scope:s.scope,state:s.active?.state??'none',script:s.active?.scriptURL}))))`,
    returnByValue: true, awaitPromise: true,
  });
  const sws = JSON.parse(swR.value || '[]');
  console.log(`[STORAGE] Service Workers: ${sws.length}`);
  for (const sw of sws) console.log(`[STORAGE] SW: ${sw.scope} state=${sw.state} script=${sw.script}`);

  // ── Cache Storage ─────────────────────────────────────────────────────────
  const { result: cacheR } = await cdp.send('Runtime.evaluate', {
    expression: `caches.keys().then(names=>Promise.all(names.map(n=>caches.open(n).then(c=>c.keys().then(k=>({name:n,count:k.length})))))).then(JSON.stringify)`,
    returnByValue: true, awaitPromise: true,
  });
  const cachesData = JSON.parse(cacheR.value || '[]');
  console.log(`[STORAGE] Cache Storage: ${cachesData.length} caches`);
  for (const c of cachesData) console.log(`[STORAGE] Cache "${c.name}": ${c.count} entries`);

  // ── Quota ─────────────────────────────────────────────────────────────────
  const { result: quotaR } = await cdp.send('Runtime.evaluate', {
    expression: `navigator.storage.estimate().then(e=>JSON.stringify({usedKB:Math.round(e.usage/1024),quotaMB:Math.round(e.quota/1024/1024),pct:((e.usage/e.quota)*100).toFixed(2)+'%'}))`,
    returnByValue: true, awaitPromise: true,
  });
  console.log(`[STORAGE] Quota: ${quotaR.value}`);

  // ── Cookie resurrection detection ─────────────────────────────────────────
  // Checks whether tracking IDs exist in both cookies AND localStorage (persistence pattern)
  const cookieNames = cookies.map(c => c.name);
  for (const { k } of ls) {
    if (cookieNames.includes(k))
      console.log(`[FINDING] COOKIE_RESURRECTION: "${k}" duplicated in both cookies and localStorage — tracking persistence pattern`);
  }
}
```

**Output prefixes:** `[STORAGE]` `[SECURITY]` `[FINDING]`

**Key findings to watch:**
- `COOKIE_NO_HTTPONLY` / `COOKIE_NO_SECURE` — tracking cookies readable by JS or transmittable over HTTP
- `SENSITIVE_IN_STORAGE` — token/auth/JWT keys in localStorage
- `COOKIE_RESURRECTION` — IDs stored in both cookies and localStorage (e.g. cross-domain tracker IDs)
- Large IndexedDB stores or Cache Storage entries indicate PWA / offline capability

**Combine with:** `security` (CSP, headers, POST body scan), `consent` (GDPR pre-grant check), `supply-chain` (third-party domain inventory).


## consent

**Trigger phrases:** "GDPR", "consent", "tracking", "privacy", "CMP", "Usercentrics", "OneTrust", "Cookiebot", "cookie banner", "ad consent", "is consent required", "what is tracked", "analytics opt-out", "adStorage", "pre-granted"

**Purpose:** Audit whether a Consent Management Platform (CMP) is present, whether consent is properly gated before trackers fire, and whether stored consent state matches what was granted. Works across Usercentrics, OneTrust, CookieYes, Cookiebot, TrustArc, and custom CMPs.

**Domains:** `Network.enable`, `Runtime.enable`, `Page.enable`

**Key signals:**
- Analytics and ad trackers firing before consent dialog appears → violation
- `ucData` / `_uetsid` / `_gcl_au` in localStorage with consent already granted → pre-grant
- `window.dataLayer` populated before consent → GTM firing too early
- `window.UC_UI` / `window.__tcfapi` / `window.Optanon` → CMP present
- Consent string in localStorage (`ucString` = Usercentrics, `eupubconsent-v2` = IAB TCF v2)

**Script skeleton:**

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Page.enable', {});
  cdp.on('Page.javascriptDialogOpening', () =>
    cdp.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {}));

  // Track tracker requests BEFORE consent can fire
  const trackerHits = [];
  cdp.on('Network.requestWillBeSent', ({ request, timestamp }) => {
    const url = request.url;
    if (/googletagmanager|google-analytics|clarity\.ms|bat\.bing|fbq|meta\.net|doubleclick|twitter|ads-twitter/i.test(url))
      trackerHits.push({ url: url.substring(0, 100), ts: timestamp });
  });

  await cdp.send('Page.navigate', { url: 'https://TARGET_URL/' });
  await new Promise(r => setTimeout(r, 6000));

  // ── CMP detection ─────────────────────────────────────────────────────────
  const { result: cmpR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      usercentrics: !!window.UC_UI || !!window.usercentrics,
      onetrust:     !!window.OneTrust || !!window.Optanon,
      cookiebot:    !!window.CookieConsent || !!window.Cookiebot,
      trustArc:     !!window.truste,
      iabTCF:       typeof window.__tcfapi === 'function',
      gtmLoaded:    Array.isArray(window.dataLayer),
      dlEvents:     (window.dataLayer || []).length,
    })`,
    returnByValue: true,
  });
  const cmp = JSON.parse(cmpR.value || '{}');
  console.log(`[SECURITY] CMP detected: ${JSON.stringify(cmp)}`);
  if (!Object.values(cmp).slice(0,5).some(Boolean)) console.log('[FINDING] NO_CMP: no consent management platform detected');

  // ── Consent state in storage ───────────────────────────────────────────────
  const { result: csR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      ucData:      localStorage.getItem('ucData'),
      ucString:    !!localStorage.getItem('ucString'),
      iabConsent:  localStorage.getItem('eupubconsent-v2'),
      otConsent:   localStorage.getItem('OptanonConsent'),
    })`,
    returnByValue: true,
  });
  const cs = JSON.parse(csR.value || '{}');
  if (cs.ucData) {
    try {
      const uc = JSON.parse(cs.ucData);
      const gcm = uc.gcm || {};
      console.log(`[SECURITY] Usercentrics GCM: ${JSON.stringify(gcm)}`);
      if (gcm.adStorage === 'granted')        console.log('[FINDING] CONSENT_PRE_GRANTED: adStorage=granted without user interaction');
      if (gcm.adPersonalization === 'granted') console.log('[FINDING] CONSENT_PRE_GRANTED: adPersonalization=granted');
      if (gcm.analyticsStorage === 'granted') console.log('[FINDING] CONSENT_PRE_GRANTED: analyticsStorage=granted');
    } catch(_) {}
  }
  if (cs.iabConsent) console.log(`[SECURITY] IAB TCF v2 string present: ${cs.iabConsent.substring(0, 40)}...`);

  // ── Trackers that fired before any consent interaction ─────────────────────
  console.log(`[SECURITY] Tracker requests on cold load: ${trackerHits.length}`);
  for (const h of trackerHits) console.log(`[SECURITY] Tracker fired: ${h.url}`);
  if (trackerHits.length > 0) console.log(`[FINDING] TRACKERS_BEFORE_CONSENT: ${trackerHits.length} tracker requests fired before consent could be shown`);

  // ── dataLayer events (GTM) ────────────────────────────────────────────────
  const { result: dlR } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify((window.dataLayer||[]).slice(0,10).map(e=>e.event||JSON.stringify(e).slice(0,60)))`,
    returnByValue: true,
  });
  const dlEvents = JSON.parse(dlR.value || '[]');
  console.log(`[SECURITY] dataLayer events: ${dlEvents.join(', ')}`);
}
```

**Output prefixes:** `[SECURITY]` `[FINDING]`

**Key findings to watch:**
- `NO_CMP` — no consent platform detected; all tracking is ungated
- `CONSENT_PRE_GRANTED` — adStorage/adPersonalization/analyticsStorage pre-granted (GDPR risk in EU)
- `TRACKERS_BEFORE_CONSENT` — trackers fire on first load before banner can appear
- Large `dataLayer` event count suggests aggressive GTM firing

**Combine with:** `storage` (full cookie + localStorage inventory), `supply-chain` (enumerate all third-party domains loaded), `network` (capture timing of tracker requests relative to page load).


## Output Prefix Reference

| Prefix | Intent categories | Meaning |
|---|---|---|
| `[NETWORK]` | network, security, supply-chain, websocket | HTTP request/response |
| `[NETWORK_ERROR]` | network, security | 4xx/5xx status |
| `[NETWORK_FAILED]` | network | Blocked/failed request |
| `[CONSOLE:ERROR]` | console | `console.error()` call |
| `[CONSOLE:WARN]` | console | `console.warn()` call |
| `[EXCEPTION]` | console | Uncaught JS exception |
| `[EXCEPTION_LOCATION]` | console | Stack frame of exception |
| `[LOG:ERROR]` | console | Browser Log domain error |
| `[PERFORMANCE]` | performance, memory | Metric value |
| `[DOM]` | dom, accessibility | DOM structure info |
| `[CSS]` | css-coverage | CSS rule info |
| `[SECURITY]` | security, supply-chain | Security-specific finding |
| `[METRIC]` | all | Summary count or measurement |
| `[SCREENSHOT]` | screenshot | Path to saved file |
| `[FINDING]` | all | Actionable issue — emit for user-relevant findings |
| `[AUTOMATE]` | automate, login | Step executed in a flow |
| `[AUTH]` | user-auth | Auth polling status update (progress line) |
| `[AUTH_COMPLETE]` | user-auth | Authentication detected — agent may proceed |
| `[AUTH_TIMEOUT]` | user-auth | Timed out waiting for auth — agent must handle |
| `[SCRAPE]` | scrape | Extracted data item |
| `[EMULATE]` | emulate | Environment override active |
| `[INJECT]` | inject | Script injected into document |
| `[MONITOR]` | monitor | State snapshot from polling loop |
| `[ACTION]` | debug, automate | Concrete next step for agent or developer |
