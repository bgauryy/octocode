# CDP Browser Surface Pattern Details

## WebSocket Surveillance

```js
export async function run(cdp) {
  await cdp.send('Network.enable', {});

  const sockets = new Map();
  let frameCount = 0;
  let socketCount = 0;
  const pageHost = (() => { try { return new URL(cdp.targetInfo.url).hostname; } catch { return ''; } })();

  cdp.on('Network.webSocketCreated', ({ requestId, url }) => {
    sockets.set(requestId, url);
    socketCount++;
    console.log(`[NETWORK] WS opened: ${url}`);
    try {
      const host = new URL(url).hostname;
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
  console.log(`[METRIC] WS sockets seen: ${socketCount}  Total frames: ${frameCount}`);
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
      const term = ${JSON.stringify(SEARCH_TERM.toLowerCase())};
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

## Save Files Screenshots PDFs and Metadata

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

## Shadow DOM Querying Inside Shadow Roots

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
