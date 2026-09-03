# CDP Quick Reference (Chrome 150 — 57 domains)

Official docs: `https://chromedevtools.github.io/devtools-protocol/tot/<Domain>/`
Live: `curl -s http://localhost:9222/json/protocol`

## Enable order (non-negotiable)

1. Enable domains → attach listeners → navigate/act
2. `DOM.enable` BEFORE `CSS.enable`
3. `Debugger.enable` → immediately `setSkipAllPauses({skip:true})`
4. `Fetch.enable` requires `patterns:[{urlPattern,requestStage}]` — no zero-arg
5. `Target.setAutoAttach` with `flatten:true` before navigation for workers

## Most-used methods per domain

**Page** — navigate, captureScreenshot, getFrameTree, createIsolatedWorld, addScriptToEvaluateOnNewDocument, setBypassCSP, handleJavaScriptDialog, loadEventFired
**DOM** — getDocument, querySelector, querySelectorAll, performSearch+getSearchResults (cross-frame), describeNode, getOuterHTML, getAttributes, setAttributeValue, getBoxModel, scrollIntoViewIfNeeded
**Runtime** — evaluate, callFunctionOn, addBinding (JS→CDP channel), getProperties, executionContextCreated, exceptionThrown, bindingCalled
**Network** — enable, getCookies(urls), getAllCookies, setCookie, deleteCookies, getResponseBody, emulateNetworkConditions, setBlockedURLs, requestWillBeSent, responseReceived, loadingFailed, webSocket*
**Fetch** — enable(patterns), requestPaused, continueRequest, fulfillRequest, failRequest
**Emulation** — setDeviceMetricsOverride, setUserAgentOverride, setTouchEmulationEnabled, setGeolocationOverride, setEmulatedMedia, setCPUThrottlingRate
**Target** — setAutoAttach(flatten:true), setDiscoverTargets, getTargets, attachToTarget, targetCreated, attachedToTarget, detachedFromTarget
**ServiceWorker** — enable, workerRegistrationUpdated, workerVersionUpdated, skipWaiting, unregister
**CSS** — enable(after DOM), startRuleUsageTracking, stopRuleUsageTracking, getComputedStyleForNode, getMatchedStylesForNode
**Profiler** — enable, startPreciseCoverage, takePreciseCoverage, stopPreciseCoverage
**HeapProfiler** — enable, takeHeapSnapshot(→addHeapSnapshotChunk events), startSampling
**Memory** — getDOMCounters (fast, no enable), prepareForLeakDetection
**Performance** — enable, getMetrics → JSHeapUsedSize, JSHeapTotalSize, Nodes, LayoutCount, ScriptDuration
**Accessibility** — enable, getFullAXTree(depth:-1), getPartialAXTree, getChildAXNodes
**Security** — enable, visibleSecurityStateChanged
**Log** — enable, entryAdded(source,level,text,url,lineNumber)
**Input** — dispatchMouseEvent, dispatchKeyEvent, insertText (no enable needed)
**DOMDebugger** — getEventListeners(objectId), setBreakpointForEventListener, setDOMBreakpoint
**Storage** — getCookies, setCookies, clearCookies, getUsageAndQuota, clearDataForOrigin
**DOMStorage** — enable, getDOMStorageItems, setDOMStorageItem
**IndexedDB** — requestDatabaseNames, requestDatabase, requestDataForObjectStore
**CacheStorage** — requestCacheNames, requestEntries, deleteCache
**Browser** — grantPermissions(geolocation,notifications,camera), getVersion, setWindowBounds
**Tracing** — start, end, dataCollected, tracingComplete (no enable)
**Overlay** — enable, highlightNode, setShowGridOverlays (DevTools visual aids)
**Debugger** — enable+setSkipAllPauses, scriptParsed, paused, getScriptSource, setBreakpoint

## Network throttle presets

```
slow3g:  {offline:false, downloadThroughput:50000,  uploadThroughput:20000,  latency:400}
fast3g:  {offline:false, downloadThroughput:180000, uploadThroughput:84000,  latency:100}
offline: {offline:true,  downloadThroughput:0,      uploadThroughput:0,      latency:0}
reset:   {offline:false, downloadThroughput:-1,     uploadThroughput:-1,     latency:0}
```

## Device presets (use before navigate)

```
iPhone 15 Pro: width:393, height:852,  dpr:3,     mobile:true
Pixel 7:       width:412, height:915,  dpr:2.625, mobile:true
iPad Air:      width:820, height:1180, dpr:2,     mobile:true
Desktop HD:    width:1920,height:1080, dpr:1,     mobile:false
```

## Worker routing (flat session model)

With `flatten:true` in `Target.setAutoAttach`, worker sessions share the main WS.
Pass `sessionId` as 4th arg in `scheme:"raw"` to route commands to a worker session:
```
scheme:"raw", method:"Runtime.evaluate", params:{...}, sessionId:"<workerSessionId>"
```
`sessionId` comes from the `Target.attachedToTarget` event.
