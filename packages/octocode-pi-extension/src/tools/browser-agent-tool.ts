/** Browser task routing and worker prompt construction for the agent browser profile. */

// ─── CDP domain reference — embedded subset for subagent bootstrapping ────────

const CDP_DOMAINS_CHROME150 = `
## CDP Domains on Chrome 150 (57 total)

### Core (stable, most useful)
- DOM (53 cmd, 19 ev) — querySelector, querySelectorAll, performSearch, getDocument, getOuterHTML, describeNode
- Runtime (stable) — evaluate, callFunctionOn, addBinding, executionContextCreated, getProperties
- Network (stable) — enable, getCookies, getAllCookies, getResponseBody, emulateNetworkConditions
- Page (stable) — navigate, captureScreenshot, getFrameTree, createIsolatedWorld, addScriptToEvaluateOnNewDocument
- Emulation (47 cmd) — setDeviceMetricsOverride, setUserAgentOverride, setTouchEmulationEnabled, setGeolocationOverride, setEmulatedMedia
- Fetch (stable) — enable with patterns, requestPaused, continueRequest, fulfillRequest
- Input (stable) — dispatchMouseEvent, dispatchKeyEvent, insertText
- Target (stable) — setAutoAttach, getTargets, attachToTarget, setDiscoverTargets
- ServiceWorker — enable, workerRegistrationUpdated, workerVersionUpdated, skipWaiting, unregister
- Storage (stable) — getCookies, setCookies, clearCookies, getUsageAndQuota
- Log (stable) — enable, entryAdded
- Performance (stable) — enable, getMetrics
- Security (stable) — enable, visibleSecurityStateChanged
- Inspector — detached, targetCrashed
- Browser (20 cmd) — grantPermissions, getVersion, getWindowBounds, setWindowBounds

### Inspection/Profiling
- CSS (39 cmd) — enable (after DOM.enable), startRuleUsageTracking, stopRuleUsageTracking, getComputedStyleForNode
- Accessibility (exp) — enable, getFullAXTree, getPartialAXTree, getChildAXNodes
- HeapProfiler — enable, takeHeapSnapshot, startSampling
- Profiler — enable, startPreciseCoverage, takePreciseCoverage, stopPreciseCoverage
- Memory — getDOMCounters, prepareForLeakDetection
- DOMDebugger — setBreakpointForEventListener, getEventListeners
- Debugger — enable + ALWAYS setSkipAllPauses({skip:true}), scriptParsed, paused
- LayerTree (exp) — enable, layerPainted, layerTreeDidChange

### Experimental/Specialty
- Tracing — start, end, dataCollected, tracingComplete
- Animation (exp) — enable, animationCreated
- Audits (exp) — enable, issueAdded (DevTools Issues panel)
- WebAudio (exp) — enable, contextCreated, contextChanged
- IndexedDB (exp) — requestDatabase, requestDataForObjectStore, deleteDatabase
- CacheStorage (exp) — requestCacheNames, requestEntries, deleteCache
- DOMStorage (exp) — enable, domStorageItemAdded, domStorageItemUpdated
- BackgroundService (exp) — startObserving, backgroundServiceEventReceived
- Extensions (exp) — loadUnpacked, getStorageItems
- FedCm (exp) — enable, dialogShown
- Media (exp) — enable, playerEventsAdded
- Overlay (exp) — enable, setShowGridOverlays, highlightNode
- PWA (exp) — getOsAppState, install
- Preload (exp) — enable, prefetchStatusUpdated
- SystemInfo — getInfo, getFeatureState
- WebAuthn (exp) — enable, addVirtualAuthenticator
- WebMCP (exp) — new in Chrome 150
- IO — read, close (stream handle from other domains)

### Key enable order rules
1. DOM.enable BEFORE CSS.enable (always)
2. Enable domains BEFORE attaching listeners
3. Attach listeners BEFORE navigating
4. Debugger.enable → immediately setSkipAllPauses({skip:true})
5. Fetch.enable needs patterns:[{urlPattern, requestStage}] — no zero-arg form
`;

// ─── Task → schemes routing ────────────────────────────────────────────────────

interface TaskRoute {
  pattern: RegExp;
  schemes: string[];
  cdpDomains: string[];
  contextKeys: string[];
}

const TASK_ROUTES: TaskRoute[] = [
  {
    pattern: /security|cookie|token|csp|header|xss|csrf|auth|credential|leak/i,
    schemes: ['security', 'network'],
    cdpDomains: ['Network', 'Runtime', 'DOM', 'DOMDebugger'],
    contextKeys: ['security', 'cookies', 'storage'],
  },
  {
    pattern: /performance|speed|slow|metric|lcp|cls|fid|layout|paint/i,
    schemes: ['performance'],
    cdpDomains: ['Performance', 'Tracing', 'Network', 'Runtime'],
    contextKeys: ['performance'],
  },
  {
    pattern: /coverage|unused|dead.?code|bundle/i,
    schemes: ['css-coverage', 'js-coverage'],
    cdpDomains: ['CSS', 'Profiler', 'DOM'],
    contextKeys: ['coverage'],
  },
  {
    pattern: /memory|heap|leak|node.?count|listener/i,
    schemes: ['memory'],
    cdpDomains: ['Memory', 'HeapProfiler', 'Performance'],
    contextKeys: ['memory'],
  },
  {
    pattern: /accessibility|a11y|aria|wcag|screen.?reader|alt/i,
    schemes: ['accessibility'],
    cdpDomains: ['Accessibility', 'DOM', 'Runtime'],
    contextKeys: ['accessibility'],
  },
  {
    pattern: /worker|service.?worker|pwa|offline|background.?sync|push/i,
    schemes: ['workers', 'service-worker'],
    cdpDomains: ['Target', 'ServiceWorker', 'Network'],
    contextKeys: ['workers', 'service-worker'],
  },
  {
    pattern: /storage|local.?storage|session.?storage|indexed.?db|cache.?storage|quota/i,
    schemes: ['storage'],
    cdpDomains: ['Network', 'Runtime', 'DOMStorage', 'IndexedDB', 'CacheStorage'],
    contextKeys: ['storage'],
  },
  {
    pattern: /websocket|ws.?frame|socket.?io|realtime/i,
    schemes: ['websocket'],
    cdpDomains: ['Network'],
    contextKeys: ['websocket'],
  },
  {
    pattern: /network|request|response|api|fetch|xhr|http.?error/i,
    schemes: ['network'],
    cdpDomains: ['Network', 'Fetch'],
    contextKeys: ['network'],
  },
  {
    pattern: /intercept|mock|block|fake.?response|modify.?header/i,
    schemes: ['intercept'],
    cdpDomains: ['Fetch'],
    contextKeys: ['intercept'],
  },
  {
    pattern: /dom|element|selector|query|html|structure|tree/i,
    schemes: ['dom'],
    cdpDomains: ['DOM', 'Runtime'],
    contextKeys: ['dom'],
  },
  {
    pattern: /console|error|exception|log|crash/i,
    schemes: ['console'],
    cdpDomains: ['Runtime', 'Log'],
    contextKeys: ['console'],
  },
  {
    pattern: /scrape|extract|data|harvest|collect|list.?all/i,
    schemes: ['scrape'],
    cdpDomains: ['DOM', 'Runtime'],
    contextKeys: ['dom', 'scrape'],
  },
  {
    pattern: /emulate|mobile|device|iphone|android|tablet|viewport|throttle|offline.?mode|geolocation/i,
    schemes: ['emulate'],
    cdpDomains: ['Emulation', 'Network'],
    contextKeys: ['emulate'],
  },
  {
    pattern: /inject|hook|monkey.?patch|override|bypass.?csp|script.?before/i,
    schemes: ['inject'],
    cdpDomains: ['Page', 'Runtime'],
    contextKeys: ['inject'],
  },
  {
    pattern: /consent|gdpr|tracking|cmp|cookie.?banner|onetrust|analytics/i,
    schemes: ['consent'],
    cdpDomains: ['Network', 'Runtime'],
    contextKeys: ['consent', 'storage'],
  },
  {
    pattern: /supply.?chain|third.?party|external.?script|sri|cdn|integrity/i,
    schemes: ['supply-chain'],
    cdpDomains: ['Network', 'Runtime'],
    contextKeys: ['supply-chain'],
  },
  {
    pattern: /full.?audit|audit.?all|everything|complete.?check|all.?check/i,
    schemes: ['debug', 'security', 'performance', 'accessibility'],
    cdpDomains: ['Network', 'Runtime', 'DOM', 'Performance', 'Security', 'Accessibility', 'Log'],
    contextKeys: ['security', 'performance', 'dom', 'network'],
  },
];

export function routeTask(task: string): { schemes: string[]; cdpDomains: string[]; contextKeys: string[] } {
  const matched = TASK_ROUTES.filter((r) => r.pattern.test(task));

  if (matched.length === 0) {
    return {
      schemes: ['debug', 'network', 'console'],
      cdpDomains: ['Network', 'Runtime', 'Log', 'DOM', 'Page'],
      contextKeys: ['network', 'console'],
    };
  }

  const schemes = [...new Set(matched.flatMap((m) => m.schemes))];
  const cdpDomains = [...new Set(matched.flatMap((m) => m.cdpDomains))];
  const contextKeys = [...new Set(matched.flatMap((m) => m.contextKeys))];

  return { schemes, cdpDomains, contextKeys };
}

// ─── Spawn config builder ──────────────────────────────────────────────────────

export function buildSpawnConfig(params: {
  task: string;
  url?: string;
  port: number;
  model?: string;
  cdpDomains: string[];
  skillContext: string;
  initialFindings: string[];
}): {
  systemPrompt: string;
  tools: string[];
  task: string;
  model?: string;
} {
  const domainList = params.cdpDomains.join(', ');

  const systemPrompt = [
    `You are a browser debugging specialist. Your ONLY browser tool is \`chromeDebug\`.`,
    ``,
    `## Task`,
    params.task,
    ``,
    params.url ? `## Target URL\n${params.url}` : '',
    ``,
    `## Chrome Port`,
    `${params.port} (Chrome is already running — do NOT set launch:true unless needed)`,
    ``,
    `## How to use chromeDebug`,
    `- scheme:"raw" + method:"Domain.Method" + params:{} → any CDP API call`,
    `- scheme:"debug" → combined network/console/exceptions/DOM pass`,
    `- scheme:"network" → request/response/cookies`,
    `- scheme:"console" → console + exceptions`,
    `- scheme:"dom" → document structure`,
    `- scheme:"security" → headers/CSP/cookies/storage/prototype`,
    `- scheme:"storage" → full storage snapshot`,
    `- scheme:"screenshot" → capture PNG`,
    `- scheme:"performance" → perf metrics`,
    `- scheme:"accessibility" → AX tree`,
    `- scheme:"intercept" → request mocking (Fetch domain)`,
    `- scheme:"workers" → web/service workers`,
    `- scheme:"emulate" → device/network/geolocation`,
    `- scheme:"inject" → pre-page-load script injection`,
    ``,
    `## Relevant CDP Domains for this task`,
    domainList,
    ``,
    `## CDP Reference`,
    CDP_DOMAINS_CHROME150,
    params.skillContext ? `\n## Patterns\n${params.skillContext}` : '',
    ``,
    `## Initial Findings`,
    params.initialFindings.length > 0 ? params.initialFindings.join('\n') : '(none yet — start by running the relevant scheme)',
    ``,
    `## Rules`,
    `- Emit [FINDING] for every issue found`,
    `- Emit [ACTION] for every recommended next step`,
    `- Use scheme:"raw" for any CDP domain not covered by named schemes`,
    `- Never emit token/cookie values — names and metadata only`,
    `- Be token-efficient: targeted queries over full-page scans`,
    `- sequence: enable domains → attach listeners → navigate/act → emit evidence`,
  ]
    .filter(Boolean)
    .join('\n');

  return {
    systemPrompt,
    tools: ['chromeDebug'],
    task: params.task,
    ...(params.model ? { model: params.model } : {}),
  };
}
