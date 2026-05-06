# CDP Quick Reference


## Domain Enable Map

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
| wait for network idle | `Network.enable` → track `requestWillBeSent`/`loadingFinished` pairs — see `waitForNetworkIdle` in SCRIPT_PATTERNS.md |
| wait for selector with actionability | `DOM.enable`, `Runtime.enable` → poll `getBoundingClientRect` + `getComputedStyle` — see `waitForSelector` in SCRIPT_PATTERNS.md |
| shadow DOM query / click | `DOM.enable`, `Runtime.enable` → `DOM.getDocument({pierce:true})` for tree; `Runtime.evaluate` pierce() for queries — `DOM.querySelector` does **NOT** cross shadow roots |
| fetch mocking / intercept | `Fetch.enable` with `patterns:[{urlPattern,requestStage}]` (no zero-arg form) |
| JS function coverage | `Profiler.enable` → `Profiler.startPreciseCoverage` |
| CSS rule coverage | `DOM.enable`, `CSS.enable` → `CSS.startRuleUsageTracking` |
| source map resolution (minified code) | `Debugger.enable` + `Debugger.setSkipAllPauses({skip:true})` → listen `Debugger.scriptParsed` for `sourceMapURL` → fetch/decode map → resolve positions; use `createSourceMapResolver(cdp)` from `scripts/sourcemap-resolver.mjs` |
| security events | `Security.enable` → listen for `Security.visibleSecurityStateChanged` |
| text search in JS scripts | `Debugger.enable` + immediately `Debugger.setSkipAllPauses({skip:true})` |
| tracing / timeline | `Page.enable` (Tracing needs no enable call) |
| mobile / viewport / touch | no enable — `Emulation.setDeviceMetricsOverride` + `setTouchEmulationEnabled` + `setUserAgentOverride` (call **before** navigate) |
| emulation / geolocation | no enable — call `Emulation.*` directly; geolocation also needs `Browser.grantPermissions` |
| dark mode / media queries | no enable — `Emulation.setEmulatedMedia` |
| network throttling / offline | `Network.enable` → `Network.emulateNetworkConditions` |
| automate / login / clicks | `Page.enable`, `Runtime.enable`, `Network.enable` |
| websocket surveillance | `Network.enable` |
| security audit (full) | `Network.enable`, `Runtime.enable`, `DOM.enable`, `Page.enable` |
| all / full audit | all of the above |

## Most-used methods

| Method | Key params | Returns |
|--------|-----------|---------|
| `Page.navigate` | `url` | `frameId` — fire after attaching all listeners |
| `Page.captureScreenshot` | `format:"png"` | `data` (base64) |
| `Page.handleJavaScriptDialog` | `accept:true/false` | — auto-dismiss alert/confirm |
| `Runtime.evaluate` | `expression`, `returnByValue:true` | `result.value` |
| `Runtime.callFunctionOn` | `objectId`, `functionDeclaration` | `result` |
| `Performance.getMetrics` | — | `metrics[]` → `{name,value}` |
| `DOM.getDocument` | `depth:2`, `pierce:true` for shadow roots | `root` node — shadow roots appear as nodeType 11 children |
| `DOM.setFileInputFiles` | `nodeId`, `files:string[]` (absolute paths) | — set files on `input[type="file"]`; dispatch `change`/`input` events after |
| `Network.getResponseBody` | `requestId` | `body`, `base64Encoded` |
| `Target.getTargets` | — | `targetInfos[]` → `{targetId,url,type}` |
| `Accessibility.getFullAXTree` | — | `nodes[]` → semantic tree with stable IDs |
| `Page.printToPDF` | `printBackground:true` | `data` (base64 PDF) |
| `Page.addScriptToEvaluateOnNewDocument` | `source` | `identifier` — inject before page JS |
| `Page.removeScriptToEvaluateOnNewDocument` | `identifier` | — |
| `Page.setBypassCSP` | `enabled:true/false` | — bypass Content-Security-Policy |
| `Fetch.enable` | `patterns:[{urlPattern,requestStage}]` | — intercept requests/responses |
| `Fetch.fulfillRequest` | `requestId,responseCode,body(base64)` | — return mock response |
| `Fetch.continueRequest` | `requestId` | — pass request through unchanged |
| `Browser.grantPermissions` | `permissions:["geolocation",…]` | — grant before navigator API |
| `Emulation.setDeviceMetricsOverride` | `width,height,deviceScaleFactor,mobile:true` | — full mobile viewport + DPR |
| `Emulation.setTouchEmulationEnabled` | `enabled:true, maxTouchPoints:5` | — real touch events (not mouse fallback) |
| `Emulation.setUserAgentOverride` | `userAgent, platform, userAgentMetadata` | — spoof UA + Sec-CH-UA hints |
| `Emulation.setGeolocationOverride` | `latitude,longitude,accuracy` | — fake GPS coords |
| `Emulation.setEmulatedMedia` | `features:[{name,value}]` | — e.g. `prefers-color-scheme: dark` |
| `Emulation.clearDeviceMetricsOverride` | — | — restore desktop viewport |
| `Network.emulateNetworkConditions` | `offline,downloadThroughput,uploadThroughput,latency` | — throttle or go offline; `-1` = reset |
| `Profiler.startPreciseCoverage` | `detailed:true` | — start JS function coverage |
| `Profiler.takePreciseCoverage` | — | `result[]` → `{url,functions[]}` |
| `CSS.startRuleUsageTracking` | — | — start CSS rule coverage |
| `CSS.stopRuleUsageTracking` | — | `ruleUsage[]` → `{used:bool}` |
| `Tracing.start` | `categories` | — start performance trace |
| `Tracing.end` | — | fires `Tracing.tracingComplete` + `Tracing.dataCollected` |

## Output prefixes

Full semantics and intent→prefix mapping → **`INTENTS.md` → Output Prefix Reference** (bottom of file).

Quick list: `[FINDING]` `[ACTION]` `[METRIC]` `[NETWORK]` `[NETWORK_ERROR]` `[NETWORK_FAILED]` `[EXCEPTION]` `[EXCEPTION_LOCATION]` `[CONSOLE:TYPE]` `[LOG:LEVEL]` `[PERFORMANCE]` `[DOM]` `[CSS]` `[SECURITY]` `[SCREENSHOT]` `[SCRAPE]` `[EMULATE]` `[AUTOMATE]` `[INJECT]` `[MONITOR]` `[SEARCH]` `[AUTH]` `[AUTH_COMPLETE]` `[AUTH_TIMEOUT]`

## Debugger domain gotcha

When `Debugger.enable` is active, any `debugger` statement or breakpoint **blocks all `Runtime.evaluate` calls** indefinitely. Add this immediately after `Debugger.enable`:

```js
await cdp.send('Debugger.enable', {});
await cdp.send('Debugger.setSkipAllPauses', { skip: true }); // prevents breakpoint hangs
```

## Event ordering constraint
Enable domains → attach `cdp.on(...)` → navigate / evaluate. **Events fire and are gone** — a listener registered after navigation misses prior events silently. This is a CDP protocol constraint, not a style preference. See `SKILL.md` → CDP Constraints.

## Dialog guard
Add when navigating pages that may open `alert()`/`confirm()`/`prompt()` dialogs (blocks all CDP commands until handled):
```js
cdp.on('Page.javascriptDialogOpening', () =>
  cdp.send('Page.handleJavaScriptDialog', { accept: true }));
```
