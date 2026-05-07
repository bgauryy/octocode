# CDP Debug Intent Details

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


**Adaptive debug recipe:**

Use this as a shape, not a fixed script. Start from `SCRIPT_PATTERNS.md` -> `Network Console`, then add only the extra probes needed for the question.

```js
export async function run(cdp) {
  const TARGET_URL = 'https://example.com';
  await cdp.send('Network.enable', {});
  await cdp.send('Runtime.enable', {});
  await cdp.send('Log.enable', {});
  await cdp.send('Page.enable', {});

  const requests = new Map();
  const findings = [];
  cdp.on('Network.requestWillBeSent', ({ requestId, request }) =>
    requests.set(requestId, { method: request.method, url: request.url }));
  cdp.on('Network.responseReceived', ({ requestId, response }) => {
    const r = requests.get(requestId);
    if (r && response.status >= 400) findings.push(`HTTP ${response.status} ${r.method} ${r.url}`);
  });
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) =>
    findings.push(exceptionDetails.exception?.description ?? exceptionDetails.text));

  await cdp.send('Page.navigate', { url: TARGET_URL });
  await new Promise(r => setTimeout(r, 5000));

  const { result } = await cdp.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      url: location.href,
      title: document.title,
      readyState: document.readyState,
      errorEls: document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length
    })`,
    returnByValue: true,
  });
  console.log(`[DEBUG] page=${result.value}`);
  for (const f of findings) console.log(`[FINDING] ${f}`);
  if (!findings.length) console.log('[ACTION] No obvious browser errors; interact and rerun if the bug is flow-dependent');
}
```

Add sourcemap, POST body, DOM, or local source tracing only when those signals would change the next action.

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

**Pattern:** Use `Network Console` pattern from `SCRIPT_PATTERNS.md`, remove console section if not needed.

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

**Pattern:** Use `Network Console` pattern from `SCRIPT_PATTERNS.md`, remove network section if not needed.

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

**Pattern:** Use `DOM Accessibility Audit` pattern from `SCRIPT_PATTERNS.md`.

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

Production bundles often mangle function names to 1-2 characters. Create `createSourceMapResolver(cdp)` before navigation, call `resolver.settle()` before analysis, and skip unresolved mangled names instead of reporting noisy dead-code findings.

**Adaptive coverage sketch:**

```js
await cdp.send('Profiler.enable', {});
await cdp.send('Profiler.startPreciseCoverage', { detailed: true, allowTriggeredUpdates: true });
// Navigate/interact, then:
const { result } = await cdp.send('Profiler.takePreciseCoverage', {});
await cdp.send('Profiler.stopPreciseCoverage', {});
await cdp.send('Profiler.disable', {});

for (const script of result) {
  if (!script.url || /node_modules|\/vendor\//i.test(script.url)) continue;
  const unused = script.functions.filter(fn => fn.ranges.every(r => r.count === 0));
  if (unused.length / Math.max(1, script.functions.length) > 0.8)
    console.log(`[FINDING] DEAD_SCRIPT: ${script.url.split('/').pop()} mostly unused`);
}
```

Report a named function only when it has a readable name or resolves through the source map. Otherwise emit a file-level metric, not dozens of mangled symbols.

**Always stop coverage when done** (already included in the pre-built script above):
```js
await cdp.send('Profiler.stopPreciseCoverage', {});
await cdp.send('Profiler.disable', {});
```
