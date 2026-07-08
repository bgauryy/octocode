/**
 * browserAgent — one tool, one skill approach.
 *
 * Architecture:
 *   chromeDebug (raw CDP execution, any domain)  ← agent uses this
 *   browserAgent (this tool) → returns spawn config + initial findings
 *   octocode-chrome-devtools skill → loaded into subagent context
 *
 * The subagent uses chromeDebug with scheme:"raw" for any CDP operation.
 * The skill teaches it what domains/methods to call.
 * This tool bootstraps that subagent with the right context.
 */

import { connectToChrome, cleanupConnection } from '../chrome-debug.js';
import { SCHEME_REGISTRY } from '../chrome-debug-schemes.js';
import type { ChromeDebugParams } from '../chrome-debug-schemes.js';
import type { ToolDefinition, ToolCallResult, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { isSubagentProcess } from './agent-tools.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

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
  schemes: Array<keyof typeof SCHEME_REGISTRY | string>;
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

function routeTask(task: string): { schemes: string[]; cdpDomains: string[]; contextKeys: string[] } {
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

function buildSpawnConfig(params: {
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

// ─── Tool registration ─────────────────────────────────────────────────────────

export function registerBrowserAgentTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  notify?: (ctx: PiContext | undefined, message: string, level?: string) => void,
): void {
  // Workers cannot spawn workers — never register this tool inside a spawned worker process.
  if (isSubagentProcess()) return;
  const setStatus = (msg: string) => {
    notify?.(undefined, msg, 'status');
  };

  registerFn(pi, registeredToolNames, {
    name: 'browserAgent',
    label: 'Browser Agent',
    description: [
      'Smart browser analysis orchestrator. Routes a natural-language task to the right CDP scheme,',
      'runs initial analysis, and returns a spawn configuration for a dedicated browser subagent.',
      '',
      'Architecture:',
      '  1. Call browserAgent({task, url}) → get findings + spawnConfig',
      '  2. Use spawnAgent({task: config.task, systemPrompt: config.systemPrompt, tools: config.tools, model: config.model})',
      '  3. The subagent uses chromeDebug with scheme:"raw" for any CDP call',
      '  4. The octocode-chrome-devtools skill is embedded in the system prompt',
      '  5. Choose config.model from `pi -ne --list-models [search]`; use the live user-configured table, not hardcoded config paths.',
      '',
      'Task routing (keyword → schemes):',
      '  security/cookie/auth    → security + network',
      '  performance/metrics     → performance',
      '  coverage/dead-code      → css-coverage + js-coverage',
      '  memory/heap/leak        → memory',
      '  accessibility/a11y      → accessibility',
      '  worker/service-worker   → workers + service-worker',
      '  storage/indexeddb       → storage',
      '  websocket               → websocket',
      '  network/request/api     → network',
      '  intercept/mock          → intercept',
      '  dom/selector/query      → dom',
      '  emulate/mobile/device   → emulate',
      '  inject/hook/patch       → inject',
      '  full-audit/everything   → debug + security + performance + accessibility',
      '  (default)               → debug + network + console',
    ].join('\n'),

    parameters: Type.Object({
      task: Type.String({
        description:
          'Natural language task. Examples: "analyze security headers and cookies", ' +
          '"check service worker lifecycle", "find all DOM elements with role=button", ' +
          '"measure JS coverage", "emulate iPhone 15 and screenshot".',
      }),
      url: Type.Optional(
        Type.String({ description: 'URL to navigate to and analyze.' }),
      ),
      port: Type.Optional(
        Type.Integer({ description: 'Chrome remote debug port (default 9222).' }),
      ),
      launch: Type.Optional(
        Type.Boolean({ description: 'Launch Chrome if not running (default false).' }),
      ),
      headless: Type.Optional(
        Type.Boolean({ description: 'Headless Chrome when launching (default true).' }),
      ),
      model: Type.Optional(
        Type.String({ description: 'Model override from `pi -ne --list-models [search]` to include in the returned spawnAgent config.' }),
      ),
      runNow: Type.Optional(
        Type.Boolean({
          description:
            'Run initial chromeDebug analysis immediately (default true). ' +
            'Set false to only get the spawn config without running.',
        }),
      ),
      durationMs: Type.Optional(
        Type.Integer({ description: 'Observation window per scheme in ms (default 5000).' }),
      ),
      workspaceCwd: Type.Optional(
        Type.String({ description: 'Workspace root for screenshots/session paths.' }),
      ),
    }),

    execute: async (
      _toolCallId: string,
      rawParams: Record<string, unknown>,
      _signal?: AbortSignal,
      _onUpdate?: unknown,
      _ctx?: PiContext,
    ): Promise<ToolCallResult> => {
      const params = rawParams as {
        task: string;
        url?: string;
        port?: number;
        launch?: boolean;
        headless?: boolean;
        model?: string;
        runNow?: boolean;
        durationMs?: number;
        workspaceCwd?: string;
      };

      const port = params.port ?? 9222;
      const runNow = params.runNow !== false;
      setStatus(`browserAgent: routing task "${params.task.slice(0, 50)}"`);

      // Route task → schemes + domains
      const { schemes, cdpDomains } = routeTask(params.task);
      const skillContext = '';

      const allLines: string[] = [];
      const schemesRun: string[] = [];

      // Run initial analysis
      if (runNow) {
        setStatus(`browserAgent: connecting to Chrome on :${port}`);

        let conn: Awaited<ReturnType<typeof connectToChrome>> | null = null;
        try {
          conn = await connectToChrome({
            port,
            launch: params.launch ?? false,
            headless: params.headless ?? true,
            workspaceCwd: params.workspaceCwd,
          });

          const session = conn.session;
          setStatus(`browserAgent: Chrome ${conn.version.Browser}`);

          // Navigate once before running schemes
          if (params.url) {
            await session.send('Page.enable', {});
            await session.send('Page.navigate', { url: params.url });
            await new Promise<void>((resolve) => {
              const timer = setTimeout(resolve, 4000);
              (session as unknown as { on(ev: string, fn: () => void): void }).on(
                'Page.loadEventFired',
                () => { clearTimeout(timer); resolve(); },
              );
            });
            allLines.push(`[AGENT] navigated to ${params.url}`);
          }

          // Run each routed scheme
          for (const scheme of schemes.slice(0, 4)) {
            const entry = SCHEME_REGISTRY[scheme as keyof typeof SCHEME_REGISTRY];
            if (!entry) continue;

            setStatus(`browserAgent: running scheme:${scheme}`);
            try {
              const result = await entry.recipe({
                session,
                params: {
                  scheme: scheme as ChromeDebugParams['scheme'],
                  url: undefined, // already navigated
                  durationMs: params.durationMs ?? 5000,
                  port,
                },
                screenshotDir: conn.screenshotDir,
                signal: AbortSignal.timeout(20_000),
              });

              if (result.evidenceLines.length > 0) {
                allLines.push(`\n[AGENT] === ${scheme.toUpperCase()} ===`);
                allLines.push(...result.evidenceLines);
                schemesRun.push(scheme);
              }
            } catch (err) {
              allLines.push(`[AGENT] ${scheme} error: ${(err as Error).message.slice(0, 100)}`);
            }
          }
        } catch (err) {
          allLines.push(`[AGENT] connect error: ${(err as Error).message}`);
        } finally {
          if (conn) {
            // browserAgent is one-shot: if it launched Chrome, terminate it so the
            // (headless) instance doesn't orphan. keepTab mirrors the launch flag.
            await cleanupConnection(conn.session, params.launch !== true, params.launch === true).catch(() => {});
          }
        }
      }

      // Build spawn config
      const spawnConfig = buildSpawnConfig({
        task: params.task,
        url: params.url,
        port,
        model: params.model,
        cdpDomains,
        skillContext,
        initialFindings: allLines,
      });

      // Token-efficient output
      const findings = allLines.filter((l) => l.includes('[FINDING]'));
      const actions = allLines.filter((l) => l.includes('[ACTION]'));

      const output = [
        `[AGENT] task: ${params.task}`,
        `[AGENT] schemes run: ${schemesRun.join(', ') || '(none)'}`,
        `[AGENT] cdp domains: ${cdpDomains.join(', ')}`,
        `[AGENT] findings: ${findings.length}  actions: ${actions.length}`,
        '',
        ...allLines,
        '',
        '=== SPAWN CONFIG ===',
        `tools: ${spawnConfig.tools.join(', ')}`,
        spawnConfig.model ? `model: ${spawnConfig.model}` : '',
        '',
        '=== SYSTEM PROMPT (pass to spawnAgent) ===',
        spawnConfig.systemPrompt,
      ].join('\n');

      return {
        content: [{ type: 'text', text: output }],
      } as ToolCallResult;
    },

    renderCall(rawParams: unknown) {
      const p = rawParams as { task?: string; url?: string };
      const raw = `browserAgent("${(p.task ?? '').slice(0, 55)}"${p.url ? ` → ${p.url}` : ''})`;
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },

    renderResult(result: unknown) {
      const r = result as { content?: Array<{ text?: string }> };
      const text = r?.content?.[0]?.text ?? '';
      const findings = (text.match(/\[FINDING\]/g) ?? []).length;
      const schemesLine = text.split('\n').find((l) => l.includes('schemes run:')) ?? '';
      const raw = `browserAgent → ${findings} findings | ${schemesLine.replace('[AGENT] ', '')}`;
      return makeRenderer((w) => [truncateToWidth(raw, w)]);
    },
  });
}
