/**
 * Chrome DevTools scheme registry for the chromeDebug Pi tool.
 *
 * Each scheme maps a debug need to: CDP domains to enable, evidence output prefixes,
 * finding rules, action rules, and a recipe function.
 *
 * MVP schemes have full implementations. Remaining schemes are stubs that point to
 * `scheme:"raw"` and the relevant CDP methods from the INTENTS_*.md knowledge base.
 *
 * Sources:
 *   _skills/octocode-chrome-devtools/references/INTENTS_DEBUG.md
 *   _skills/octocode-chrome-devtools/references/INTENTS_INSPECT.md
 *   _skills/octocode-chrome-devtools/references/INTENTS_AUTOMATION.md
 *   _skills/octocode-chrome-devtools/references/INTENTS_AUTH.md
 *   _skills/octocode-chrome-devtools/references/INTENTS_STORAGE_CONSENT.md
 *   _skills/octocode-chrome-devtools/references/INTENTS_ENVIRONMENT.md
 */

import fs from 'node:fs';
import type { CdpSession } from './chrome-debug.js';
import { redactEvidence, captureScreenshot, buildRetryMarker, isCdpError } from './chrome-debug.js';
import { assertPathAllowed } from './tools/path-guard.js';

// ─── Types ────────────────────────────────────────────────────────────────────

export const SCHEMES = [
  'debug',
  'network',
  'console',
  'dom',
  'performance',
  'screenshot',
  'intercept',
  'security',
  'storage',
  'automate',
  'live-page',
  'user-auth',
  'raw',
  // Stubs
  'memory',
  'css-coverage',
  'js-coverage',
  'websocket',
  'service-worker',
  'workers',
  'accessibility',
  'supply-chain',
  'full-audit',
  'consent',
  'scrape',
  'login',
  'emulate',
  'inject',
  'monitor',
] as const;

export type Scheme = (typeof SCHEMES)[number];

export const ACTIONS = [
  'observe',
  'capture',
  'navigate',
  'interact',
  'wait',
  'breakpoint',
  'resume',
  'screenshot',
  'eval',
  'list-targets',
  'attach',
  'cleanup',
  'raw',
] as const;

export type Action = (typeof ACTIONS)[number];

export interface ChromeDebugParams {
  scheme: Scheme;
  action?: Action;
  // Scheme-specific
  url?: string;
  selector?: string;
  expression?: string;
  interact?: { click?: string; fill?: { selector: string; value: string }; wait?: string };
  // Raw action
  method?: string;
  params?: Record<string, unknown>;
  sessionId?: string;
  // Screenshot
  format?: 'png' | 'jpeg' | 'webp' | 'pdf';
  quality?: number;
  clip?: { x: number; y: number; width: number; height: number; scale?: number };
  fullPage?: boolean;
  // Extended params
  stealth?: boolean;
  bypassCSP?: boolean;
  scriptSource?: string;
  scriptFile?: string;  // absolute path to a .mjs file — loads its exported STEALTH_SCRIPT or default export
  depth?: number;
  xpath?: string;
  // Emulate
  device?: { width: number; height: number; deviceScaleFactor: number; mobile: boolean; userAgent?: string };
  throttle?: { offline?: boolean; downloadThroughput?: number; uploadThroughput?: number; latency?: number };
  // Session/lifecycle
  durationMs?: number;
  timeoutMs?: number;
  port?: number;
  targetId?: string;
  targetUrl?: string;
  targetType?: string;
  newTab?: string;
  keepTab?: boolean;
  launch?: boolean;
  headless?: boolean;
  cleanup?: boolean;
}

export interface SchemeResult {
  evidenceLines: string[];
  details: Record<string, unknown>;
}

export interface RecipeContext {
  session: CdpSession;
  params: ChromeDebugParams;
  screenshotDir: string;
  signal?: AbortSignal;
  setStatus?: (msg: string) => void;
}

export type Recipe = (ctx: RecipeContext) => Promise<SchemeResult>;

export interface SchemeEntry {
  domains: string[];
  prefixes: string[];
  recipe: Recipe;
}

// ─── Stealth script (minimal inline evasions) ───────────────────────────────────

export const STEALTH_SCRIPT = `(function(){
  try{Object.defineProperty(navigator,'webdriver',{get:()=>undefined,configurable:true})}catch(e){}
  if(!window.chrome)window.chrome={runtime:{}};
  if(!window.chrome.runtime)window.chrome.runtime={};
  try{Object.defineProperty(navigator,'vendor',{get:()=>'Google Inc.',configurable:true})}catch(e){}
  try{Object.defineProperty(navigator,'languages',{get:()=>['en-US','en'],configurable:true})}catch(e){}
  try{if((navigator.hardwareConcurrency||0)<4)Object.defineProperty(navigator,'hardwareConcurrency',{get:()=>4,configurable:true})}catch(e){}
  try{if(navigator.plugins.length===0){const p=Object.create(Plugin.prototype);Object.defineProperty(p,'name',{value:'Chrome PDF Plugin',enumerable:true});Object.defineProperty(p,'length',{value:0,enumerable:true});Object.defineProperty(navigator,'plugins',{get:()=>{const a=[p];a.item=i=>p;a.namedItem=()=>null;a.refresh=()=>{};return a},configurable:true})}}catch(e){}
  try{const g=WebGLRenderingContext.prototype.getParameter;WebGLRenderingContext.prototype.getParameter=function(p){if(p===37445)return 'Intel Inc.';if(p===37446)return 'Intel Iris OpenGL Engine';return g.call(this,p)}}catch(e){}
  try{if(window.outerWidth===0)Object.defineProperty(window,'outerWidth',{get:()=>window.innerWidth,configurable:true})}catch(e){}
  try{if(Notification.permission==='denied')Object.defineProperty(Notification,'permission',{get:()=>'default',configurable:true})}catch(e){}
})();`;

// ─── Helpers ──────────────────────────────────────────────────────────────────

function emit(lines: string[], prefix: string, text: string): void {
  lines.push(redactEvidence(`${prefix} ${text}`));
}

async function navigateAndWait(
  session: CdpSession,
  url: string,
  waitMs = 4000,
  signal?: AbortSignal,
): Promise<void> {
  await session.send('Page.enable', {});

  // Guard 1: Dismiss JS dialogs (alert/confirm/prompt) that would block CDP indefinitely.
  (session as unknown as { on: (ev: string, fn: () => void) => void }).on(
    'Page.javascriptDialogOpening',
    () => { session.send('Page.handleJavaScriptDialog', { accept: false }).catch(() => {}); },
  );

  // Guard 2: Skip debugger statements — sites with `debugger;` pause all CDP evaluation.
  await session.send('Debugger.enable', {}).catch(() => {});
  await session.send('Debugger.setSkipAllPauses', { skip: true }).catch(() => {});

  await session.send('Page.navigate', { url });

  // Wait for load event or timeout
  await new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, waitMs);
    const handler = () => { clearTimeout(timer); resolve(); };
    session.on('Page.loadEventFired', handler);
    signal?.addEventListener('abort', () => { clearTimeout(timer); resolve(); }, { once: true });
  });
}

async function runInteract(
  session: CdpSession,
  interact: ChromeDebugParams['interact'],
  lines: string[],
): Promise<void> {
  if (!interact) return;

  if (interact.wait) {
    const waitMs = parseInt(interact.wait, 10) || 2000;
    await new Promise((r) => setTimeout(r, waitMs));
    emit(lines, '[AUTOMATE]', `waited ${waitMs}ms`);
  }

  if (interact.fill) {
    const { selector, value } = interact.fill;
    await session.send('Runtime.evaluate', {
      expression: `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if(el){ const nv = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value'); nv?.set?.call(el, ${JSON.stringify(value)}); el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true})); return 'filled'; } return 'not found'; })()`,
      returnByValue: true,
    });
    emit(lines, '[AUTOMATE]', `fill ${selector} = <value-redacted>`);
  }

  if (interact.click) {
    const selector = interact.click;
    await session.send('Runtime.evaluate', {
      expression: `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if(el){ el.click(); return 'clicked'; } return 'not found'; })()`,
      returnByValue: true,
    });
    emit(lines, '[AUTOMATE]', `click ${selector}`);
  }
}

// ─── MVP Scheme: debug ────────────────────────────────────────────────────────

const debugRecipe: Recipe = async ({ session, params, screenshotDir, signal }) => {
  const lines: string[] = [];
  const findings: Array<{ priority: number; line: string; action: string }> = [];

  // Enable domains
  await session.send('Network.enable', {});
  await session.send('Runtime.enable', {});
  await session.send('Log.enable', {});
  await session.send('Page.enable', {});
  await session.send('DOM.enable', {});

  const requests = new Map<string, { method: string; url: string }>();

  session.on('Network.requestWillBeSent', ({ requestId, request }) => {
    const req = request as { method?: string; url?: string };
    if (typeof requestId === 'string') {
      requests.set(requestId, { method: req.method ?? 'GET', url: req.url ?? '' });
    }
  });

  session.on('Network.responseReceived', ({ requestId, response }) => {
    const r = requests.get(requestId as string);
    const resp = response as { status?: number };
    if (r && resp.status !== undefined && resp.status >= 400) {
      findings.push({
        priority: 2,
        line: `[FINDING] HTTP_ERROR: ${resp.status} ${r.method} ${r.url}`,
        action: `[ACTION] check handler for ${r.method} ${r.url} — returned ${resp.status}`,
      });
    }
  });

  session.on('Network.loadingFailed', ({ requestId, errorText }) => {
    const r = requests.get(requestId as string);
    if (!r) return; // skip internal Chrome requests not in our map
    const url = r.url;
    findings.push({
      priority: 3,
      line: `[FINDING] BLOCKED: ${url} — ${errorText ?? 'failed'}`,
      action: `[ACTION] check CORS / network config for ${url}`,
    });
  });

  session.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const ex = exceptionDetails as Record<string, unknown>;
    const exception = ex['exception'] as Record<string, unknown> | undefined;
    const desc = String(exception?.['description'] ?? ex['text'] ?? 'unknown exception');
    const loc = `${ex['url'] ?? ''}:${ex['lineNumber'] ?? ''}`;
    findings.push({
      priority: 1,
      line: `[FINDING] EXCEPTION: ${desc.slice(0, 200)} at ${loc}`,
      action: `[ACTION] search "${desc.slice(0, 60)}" in localSearchCode — exception at ${loc}`,
    });
  });

  session.on('Runtime.consoleAPICalled', ({ type, args }) => {
    if (type !== 'error' && type !== 'warn') return;
    const argList = args as Array<{ value?: unknown; description?: string }>;
    const msg = argList.map((a) => String(a.value ?? a.description ?? '')).join(' ').slice(0, 200);
    findings.push({
      priority: 4,
      line: `[FINDING] CONSOLE_ERROR: ${msg}`,
      action: `[ACTION] search "${msg.slice(0, 60)}" in localSearchCode`,
    });
  });

  // Navigate if URL provided
  if (params.url) {
    emit(lines, '[DEBUG]', `navigating to ${params.url}`);
    await navigateAndWait(session, params.url, 4000, signal);
  }

  // Interact if requested
  if (params.interact) {
    await runInteract(session, params.interact, lines);
    // Give events time to fire
    await new Promise((r) => setTimeout(r, 2000));
  }

  // DOM state
  let pageInfo: { url?: string; title?: string; readyState?: string; errorEls?: number } = {};
  try {
    const evalResult = await session.send('Runtime.evaluate', {
      expression: `JSON.stringify({url:location.href,title:document.title,readyState:document.readyState,errorEls:document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length})`,
      returnByValue: true,
    });
    pageInfo = JSON.parse(String((evalResult['result'] as Record<string, unknown>)?.['value'] ?? '{}'));
  } catch { /* page not ready */ }

  // OBSERVE block
  emit(lines, '[DEBUG]', '=== OBSERVE ===');
  emit(lines, '[DEBUG]', `Page: ${pageInfo.title ?? 'unknown'} | readyState: ${pageInfo.readyState ?? 'unknown'}`);
  emit(lines, '[DEBUG]', `URL: ${pageInfo.url ?? params.url ?? 'unknown'}`);

  const exceptions = findings.filter((f) => f.priority === 1).length;
  const httpErrors = findings.filter((f) => f.priority === 2).length;
  const blocked = findings.filter((f) => f.priority === 3).length;
  const consoleErrors = findings.filter((f) => f.priority === 4).length;
  emit(lines, '[DEBUG]', `Exceptions: ${exceptions}  Console errors: ${consoleErrors}  Network errors: ${httpErrors}  Blocked: ${blocked}`);

  // Emit sorted findings
  findings.sort((a, b) => a.priority - b.priority);
  for (const f of findings) {
    lines.push(redactEvidence(f.line));
  }

  if (pageInfo.errorEls && pageInfo.errorEls > 0) {
    findings.push({
      priority: 5,
      line: `[FINDING] DOM_ERROR_STATE: ${pageInfo.errorEls} error-state elements visible`,
      action: `[ACTION] inspect DOM for .error / [aria-invalid] — user-visible errors present`,
    });
    lines.push(redactEvidence(`[FINDING] DOM_ERROR_STATE: ${pageInfo.errorEls} error-state elements visible`));
  }

  if (pageInfo.readyState && pageInfo.readyState !== 'complete') {
    emit(lines, '[FINDING]', `PAGE_NOT_READY: readyState=${pageInfo.readyState}`);
  }

  if (requests.size === 0 && params.url) {
    emit(lines, '[FINDING]', 'NO_REQUESTS: page may be offline or blocked');
  }

  // ACT block
  emit(lines, '[DEBUG]', '=== ACT ===');
  if (findings.length === 0) {
    emit(lines, '[ACTION]', 'No errors detected — interact with the page (click, submit form) and re-run debug');
  } else {
    for (const f of findings) {
      lines.push(redactEvidence(f.action));
    }
  }

  // Optional screenshot
  if (params.interact || params.url) {
    try {
      const ss = await captureScreenshot(session, {
        screenshotDir,
        scheme: 'debug',
        targetUrl: params.url ?? params.targetUrl,
      });
      lines.push(ss.evidenceLine);
    } catch { /* screenshot is best-effort */ }
  }

  return {
    evidenceLines: lines,
    details: {
      findings: findings.map((f) => f.line),
      pageInfo,
      requestCount: requests.size,
    },
  };
};

// ─── MVP Scheme: network ──────────────────────────────────────────────────────

const networkRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];

  await session.send('Network.enable', {});

  const requests = new Map<string, { method: string; url: string; startTime: number }>();
  const findings: string[] = [];

  session.on('Network.requestWillBeSent', ({ requestId, request, timestamp }) => {
    const req = request as { method?: string; url?: string };
    if (typeof requestId === 'string') {
      requests.set(requestId, {
        method: req.method ?? 'GET',
        url: req.url ?? '',
        startTime: (timestamp as number) ?? 0,
      });
    }
  });

  session.on('Network.responseReceived', ({ requestId, response }) => {
    const r = requests.get(requestId as string);
    const resp = response as { status?: number; mimeType?: string };
    if (!r) return;
    const status = resp.status ?? 0;
    if (status >= 400) {
      findings.push(redactEvidence(`[FINDING] HTTP_ERROR: ${status} ${r.method} ${r.url}`));
      findings.push(redactEvidence(`[ACTION] check handler for ${r.method} ${r.url} — returned ${status}`));
    }
    emit(lines, '[NETWORK]', `${status} ${r.method} ${r.url}`);
  });

  session.on('Network.loadingFailed', ({ requestId, errorText }) => {
    const r = requests.get(requestId as string);
    if (!r) return; // skip internal Chrome requests not in our map
    const url = r.url;
    findings.push(redactEvidence(`[FINDING] BLOCKED: ${url} — ${errorText ?? 'failed'}`));
    findings.push(redactEvidence(`[ACTION] check CORS / network config for ${url}`));
    emit(lines, '[NETWORK_FAILED]', `${url} — ${String(errorText ?? 'failed')}`);
  });

  if (params.url) {
    await navigateAndWait(session, params.url, params.durationMs ?? 4000, signal);
  } else {
    // Monitor for durationMs
    const wait = params.durationMs ?? 5000;
    await new Promise((r) => {
      const t = setTimeout(r, wait);
      signal?.addEventListener('abort', () => { clearTimeout(t); r(undefined); }, { once: true });
    });
  }

  emit(lines, '[NETWORK]', `=== Summary: ${requests.size} requests ===`);
  for (const f of findings) lines.push(f);

  return {
    evidenceLines: lines,
    details: { requestCount: requests.size, findings },
  };
};

// ─── MVP Scheme: console ──────────────────────────────────────────────────────

const consoleRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const findings: string[] = [];

  await session.send('Runtime.enable', {});
  await session.send('Log.enable', {});

  session.on('Runtime.consoleAPICalled', ({ type, args }) => {
    const argList = args as Array<{ value?: unknown; description?: string }>;
    const msg = argList.map((a) => String(a.value ?? a.description ?? '')).join(' ');
    const t = String(type);
    emit(lines, `[CONSOLE:${t.toUpperCase()}]`, redactEvidence(msg.slice(0, 300)));
    if (t === 'error' || t === 'warn') {
      findings.push(redactEvidence(`[FINDING] CONSOLE_${t.toUpperCase()}: ${msg.slice(0, 200)}`));
    }
  });

  session.on('Log.entryAdded', ({ entry }) => {
    const e = entry as { level?: string; text?: string; source?: string };
    if (e.level === 'error' || e.level === 'warning') {
      emit(lines, `[LOG:${e.level?.toUpperCase()}]`, redactEvidence(String(e.text ?? '').slice(0, 300)));
    }
  });

  session.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const ex = exceptionDetails as Record<string, unknown>;
    const exception = ex['exception'] as Record<string, unknown> | undefined;
    const desc = String(exception?.['description'] ?? ex['text'] ?? 'exception');
    emit(lines, '[EXCEPTION]', redactEvidence(desc.slice(0, 300)));
    emit(lines, '[EXCEPTION_LOCATION]', `${ex['url'] ?? ''}:${ex['lineNumber'] ?? ''}:${ex['columnNumber'] ?? ''}`);
    findings.push(redactEvidence(`[FINDING] EXCEPTION: ${desc.slice(0, 200)}`));
  });

  if (params.url) {
    await navigateAndWait(session, params.url, params.durationMs ?? 5000, signal);
  } else {
    const wait = params.durationMs ?? 3000;
    await new Promise((r) => {
      const t = setTimeout(r, wait);
      signal?.addEventListener('abort', () => { clearTimeout(t); r(undefined); }, { once: true });
    });
  }

  for (const f of findings) lines.push(f);

  return { evidenceLines: lines, details: { findings } };
};

// ─── MVP Scheme: dom ──────────────────────────────────────────────────────────

const domRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];

  await session.send('DOM.enable', {});
  await session.send('Runtime.enable', {});

  if (params.url) {
    await navigateAndWait(session, params.url);
  }

  // Get document
  const docResult = await session.send('DOM.getDocument', { depth: 2 });
  const root = docResult['root'] as Record<string, unknown>;
  emit(lines, '[DOM]', `root: ${root['nodeName'] ?? 'unknown'} nodeId=${root['nodeId'] ?? ''}`);

  // Evaluate DOM queries
  const expression = params.expression ?? `JSON.stringify({
    title: document.title,
    url: location.href,
    readyState: document.readyState,
    bodyChildren: document.body ? document.body.children.length : 0,
    errorEls: document.querySelectorAll('.error,[aria-invalid="true"],[data-error]').length,
    forms: document.forms.length,
    links: document.links.length
  })`;

  const evalResult = await session.send('Runtime.evaluate', {
    expression,
    returnByValue: true,
  });
  const info = JSON.parse(String((evalResult['result'] as Record<string, unknown>)?.['value'] ?? '{}')) as Record<string, unknown>;

  for (const [k, v] of Object.entries(info)) {
    emit(lines, '[DOM]', `${k}: ${String(v)}`);
  }

  if (typeof info['errorEls'] === 'number' && info['errorEls'] > 0) {
    emit(lines, '[FINDING]', `DOM_ERROR_STATE: ${info['errorEls']} error-state elements visible`);
    emit(lines, '[ACTION]', `inspect DOM for .error / [aria-invalid] — ${info['errorEls']} error-state elements`);
  } else {
    emit(lines, '[ACTION]', 'No DOM error indicators found');
  }

  return { evidenceLines: lines, details: { domInfo: info } };
};

// ─── MVP Scheme: performance ──────────────────────────────────────────────────

const performanceRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];

  await session.send('Performance.enable', {});
  await session.send('Runtime.enable', {});

  if (params.url) {
    await navigateAndWait(session, params.url, 5000);
  }

  const metricsResult = await session.send('Performance.getMetrics', {});
  const metrics = (metricsResult['metrics'] as Array<{ name: string; value: number }>) ?? [];

  for (const { name, value } of metrics) {
    emit(lines, '[METRIC]', `${name}: ${value.toFixed(2)}`);
  }

  // Web vitals via Runtime
  const vitals = await session.send('Runtime.evaluate', {
    expression: `JSON.stringify(performance.getEntriesByType('paint').map(e=>({name:e.name,startTime:e.startTime.toFixed(1)})))`,
    returnByValue: true,
  }).catch(() => ({ result: { value: '[]' } }));

  const paintEntries = JSON.parse(String((vitals['result'] as Record<string, unknown>)?.['value'] ?? '[]')) as Array<{ name: string; startTime: string }>;
  for (const e of paintEntries) {
    emit(lines, '[METRIC]', `${e.name}: ${e.startTime}ms`);
  }

  if (metrics.length === 0 && paintEntries.length === 0) {
    emit(lines, '[ACTION]', 'No metrics captured — navigate to a page and re-run');
  }

  return {
    evidenceLines: lines,
    details: { metrics: Object.fromEntries(metrics.map((m) => [m.name, m.value])), paint: paintEntries },
  };
};

// ─── MVP Scheme: screenshot ───────────────────────────────────────────────────

const screenshotRecipe: Recipe = async ({ session, params, screenshotDir }) => {
  const lines: string[] = [];

  await session.send('Page.enable', {});

  if (params.url) {
    await navigateAndWait(session, params.url, 3000);
  }

  try {
    const ss = await captureScreenshot(session, {
      screenshotDir,
      scheme: 'screenshot',
      format: params.format ?? 'png',
      quality: params.quality,
      clip: params.clip,
      fullPage: params.fullPage,
      targetUrl: params.url ?? params.targetUrl,
    });
    lines.push(ss.evidenceLine);
    return {
      evidenceLines: lines,
      details: { screenshotPath: ss.path, format: params.format ?? 'png' },
    };
  } catch (err) {
    const msg = (err as Error).message ?? String(err);
    emit(lines, '[SCREENSHOT_ERROR]', msg);
    return { evidenceLines: lines, details: { error: msg } };
  }
};

// ─── MVP Scheme: intercept ────────────────────────────────────────────────────

const interceptRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];

  await session.send('Network.enable', {});
  await session.send('Fetch.enable', {
    patterns: [{ urlPattern: '*' }],
    handleAuthRequests: false,
  });

  const paused = new Set<string>();
  const findings: string[] = [];

  session.on('Fetch.requestPaused', async ({ requestId, request }) => {
    const req = request as { method?: string; url?: string };
    const url = req.url ?? '';
    const method = req.method ?? 'GET';
    paused.add(requestId as string);
    emit(lines, '[INTERCEPT]', `paused: ${method} ${url}`);

    // Analyze and continue (never block by default — requires explicit confirmation)
    if (/token|auth|apikey|secret/i.test(url)) {
      findings.push(redactEvidence(`[FINDING] SENSITIVE_REQUEST: ${method} ${url}`));
    }

    // Always continue paused requests (deadlock prevention)
    try {
      await session.send('Fetch.continueRequest', { requestId });
      paused.delete(requestId as string);
    } catch {
      // If continue fails, request may have already timed out
    }
  });

  if (params.url) {
    await navigateAndWait(session, params.url, params.durationMs ?? 5000, signal);
  } else {
    const wait = params.durationMs ?? 5000;
    await new Promise((r) => {
      const t = setTimeout(r, wait);
      signal?.addEventListener('abort', () => { clearTimeout(t); r(undefined); }, { once: true });
    });
  }

  // Ensure any remaining paused requests are continued
  for (const id of paused) {
    await session.send('Fetch.continueRequest', { requestId: id }).catch(() => undefined);
  }

  await session.send('Fetch.disable', {}).catch(() => undefined);

  for (const f of findings) lines.push(f);

  return { evidenceLines: lines, details: { interceptedCount: paused.size, findings } };
};

// ─── MVP Scheme: security ─────────────────────────────────────────────────────

const securityRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  const findings: string[] = [];

  await session.send('Network.enable', {});
  await session.send('Security.enable', {}).catch(() => undefined);

  if (params.url) {
    await navigateAndWait(session, params.url, 4000);
  }

  // Check cookies (names only — never values)
  try {
    const cookieResult = await session.send('Network.getCookies', {});
    const cookies = (cookieResult['cookies'] as Array<{ name?: string; secure?: boolean; httpOnly?: boolean; sameSite?: string }>) ?? [];
    for (const c of cookies) {
      const flags = [
        c.secure ? 'Secure' : 'NO-Secure',
        c.httpOnly ? 'HttpOnly' : 'NO-HttpOnly',
        `SameSite=${c.sameSite ?? 'none'}`,
      ].join(' ');
      emit(lines, '[COOKIE]', `${c.name ?? '?'} [${flags}]`);
      if (!c.secure) findings.push(`[FINDING] INSECURE_COOKIE: ${c.name ?? '?'} missing Secure flag`);
      if (!c.httpOnly) findings.push(`[FINDING] XSS_RISK_COOKIE: ${c.name ?? '?'} missing HttpOnly flag`);
    }
  } catch { /* network may not be fully ready */ }

  // Check CSP and security headers via network response
  const headerResults = await session.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      hasCSP: !!document.querySelector('meta[http-equiv="Content-Security-Policy"]'),
      hasMeta: !!document.querySelector('meta[http-equiv]'),
    })`,
    returnByValue: true,
  }).catch(() => ({ result: { value: '{}' } }));
  const headerInfo = JSON.parse(String((headerResults['result'] as Record<string, unknown>)?.['value'] ?? '{}')) as Record<string, unknown>;
  emit(lines, '[SECURITY]', `CSP meta tag: ${headerInfo['hasCSP'] ? 'present' : 'not found'}`);

  // Prototype pollution check
  const protoResult = await session.send('Runtime.evaluate', {
    expression: `JSON.stringify({ polluted: '__proto__' in {} ? 'yes' : 'no', objectPolluted: Object.prototype['__octocode_test'] !== undefined })`,
    returnByValue: true,
  }).catch(() => ({ result: { value: '{}' } }));
  const protoInfo = JSON.parse(String((protoResult['result'] as Record<string, unknown>)?.['value'] ?? '{}')) as Record<string, unknown>;
  emit(lines, '[SECURITY]', `prototype pollution check: ${JSON.stringify(protoInfo)}`);

  for (const f of findings) lines.push(f);
  if (findings.length === 0) emit(lines, '[ACTION]', 'No critical cookie security issues found — check network tab for header-level CSP/HSTS');

  return { evidenceLines: lines, details: { findings, headerInfo } };
};

// ─── MVP Scheme: storage ──────────────────────────────────────────────────────

const storageRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];

  await session.send('Network.enable', {});

  if (params.url) {
    await navigateAndWait(session, params.url, 3000);
  }

  // Cookies (names + sizes only)
  try {
    const cookieResult = await session.send('Network.getCookies', {});
    const cookies = (cookieResult['cookies'] as Array<{ name?: string }>) ?? [];
    emit(lines, '[STORAGE]', `cookies: ${cookies.length} (names: ${cookies.map((c) => c.name).join(', ')})`);
  } catch { /* ignore */ }

  // localStorage / sessionStorage (keys only — no values)
  const storageResult = await session.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      localStorageKeys: Object.keys(localStorage),
      sessionStorageKeys: Object.keys(sessionStorage),
      localStorageSize: JSON.stringify(localStorage).length,
      sessionStorageSize: JSON.stringify(sessionStorage).length
    })`,
    returnByValue: true,
  }).catch(() => ({ result: { value: '{}' } }));
  const storageInfo = JSON.parse(String((storageResult['result'] as Record<string, unknown>)?.['value'] ?? '{}')) as {
    localStorageKeys?: string[];
    sessionStorageKeys?: string[];
    localStorageSize?: number;
    sessionStorageSize?: number;
  };

  emit(lines, '[STORAGE]', `localStorage: ${storageInfo.localStorageKeys?.length ?? 0} keys, ${storageInfo.localStorageSize ?? 0}B (keys: ${(storageInfo.localStorageKeys ?? []).join(', ')})`);
  emit(lines, '[STORAGE]', `sessionStorage: ${storageInfo.sessionStorageKeys?.length ?? 0} keys, ${storageInfo.sessionStorageSize ?? 0}B`);

  // Check for cookie resurrection patterns
  const authKeys = (storageInfo.localStorageKeys ?? []).filter((k) =>
    /token|auth|jwt|session|refresh/i.test(k),
  );
  if (authKeys.length > 0) {
    emit(lines, '[FINDING]', `AUTH_STORAGE: localStorage has auth-related keys: ${authKeys.join(', ')}`);
    emit(lines, '[ACTION]', 'Review auth key usage — ensure tokens are properly invalidated on logout (COOKIE_RESURRECTION risk)');
  }

  return { evidenceLines: lines, details: { storageInfo } };
};

// ─── MVP Scheme: automate ─────────────────────────────────────────────────────

const automateRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];

  await session.send('Page.enable', {});
  await session.send('Runtime.enable', {});
  await session.send('Page.addScriptToEvaluateOnNewDocument', {
    source: 'window.__octocode_automation = true;',
  }).catch(() => undefined);

  // Dialog guard
  session.on('Page.javascriptDialogOpening', async ({ type, message }) => {
    emit(lines, '[AUTOMATE]', `dialog intercepted: ${type} — "${String(message).slice(0, 100)}"`);
    await session.send('Page.handleJavaScriptDialog', { accept: true }).catch(() => undefined);
  });

  if (params.url) {
    emit(lines, '[AUTOMATE]', `navigate → ${params.url}`);
    await navigateAndWait(session, params.url, 3000, signal);
  }

  if (params.interact) {
    await runInteract(session, params.interact, lines);
  }

  if (params.expression) {
    const exprResult = await session.send('Runtime.evaluate', {
      expression: params.expression,
      returnByValue: true,
    });
    const val = String((exprResult['result'] as Record<string, unknown>)?.['value'] ?? '');
    emit(lines, '[AUTOMATE]', `eval result: ${redactEvidence(val.slice(0, 300))}`);
  }

  emit(lines, '[ACTION]', 'Automation complete — re-run debug scheme to inspect resulting page state');

  return { evidenceLines: lines, details: {} };
};

// ─── MVP Scheme: live-page ────────────────────────────────────────────────────

const livePageRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];

  await session.send('Runtime.enable', {});

  // Do NOT navigate — inspect current page as-is
  const evalResult = await session.send('Runtime.evaluate', {
    expression: `JSON.stringify({
      url: location.href,
      title: document.title,
      readyState: document.readyState,
      bodyText: document.body ? document.body.innerText.slice(0, 500) : '',
      formCount: document.forms.length,
      inputCount: document.querySelectorAll('input,select,textarea').length,
      buttonCount: document.querySelectorAll('button,[type=submit]').length
    })`,
    returnByValue: true,
  });
  const info = JSON.parse(String((evalResult['result'] as Record<string, unknown>)?.['value'] ?? '{}')) as Record<string, unknown>;

  emit(lines, '[FINDING]', `PAGE_STATE: ${info['url'] ?? 'unknown'} | ${info['title'] ?? ''} | readyState=${info['readyState'] ?? ''}`);
  emit(lines, '[DEBUG]', `forms: ${info['formCount']}  inputs: ${info['inputCount']}  buttons: ${info['buttonCount']}`);
  if (info['bodyText']) {
    emit(lines, '[DOM]', `body text (500 chars): ${redactEvidence(String(info['bodyText']).slice(0, 500))}`);
  }

  if (params.expression) {
    const exprResult = await session.send('Runtime.evaluate', {
      expression: params.expression,
      returnByValue: true,
    });
    emit(lines, '[EVAL]', redactEvidence(String((exprResult['result'] as Record<string, unknown>)?.['value'] ?? '')));
  }

  emit(lines, '[ACTION]', 'Live page state captured — use scheme:"debug" to collect errors, scheme:"network" to watch requests');

  return { evidenceLines: lines, details: { pageInfo: info } };
};

// ─── MVP Scheme: user-auth ────────────────────────────────────────────────────

const userAuthRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];

  await session.send('Page.enable', {});
  await session.send('Network.enable', {});

  if (params.url) {
    emit(lines, '[AUTH]', `navigating to ${params.url}`);
    await navigateAndWait(session, params.url, 5000, signal);
  }

  emit(lines, '[AUTH]', 'Visible browser open — perform login in the browser window');
  emit(lines, '[AUTH]', `Waiting up to ${(params.durationMs ?? 60000) / 1000}s for auth completion (AbortSignal will cancel)`);

  // Poll for URL change indicating login completion
  const startTime = Date.now();
  const maxWait = params.durationMs ?? 60_000;
  let authCompleted = false;

  while (!authCompleted && !signal?.aborted && Date.now() - startTime < maxWait) {
    await new Promise((r) => setTimeout(r, 2000));
    if (signal?.aborted) break;

    try {
      const urlResult = await session.send('Runtime.evaluate', {
        expression: 'location.href',
        returnByValue: true,
      });
      const currentUrl = String((urlResult['result'] as Record<string, unknown>)?.['value'] ?? '');
      emit(lines, '[AUTH]', `current URL: ${currentUrl}`);

      // Detect common auth completion patterns
      if (
        !currentUrl.includes('/login') &&
        !currentUrl.includes('/signin') &&
        !currentUrl.includes('/auth') &&
        params.url &&
        currentUrl !== params.url
      ) {
        authCompleted = true;
        emit(lines, '[AUTH_COMPLETE]', `user authenticated — now at ${currentUrl}`);
        emit(lines, '[ACTION]', 'Auth complete — re-run with scheme:"security" or scheme:"storage" to inspect auth state');
        break;
      }
    } catch { /* page may be navigating */ }
  }

  if (!authCompleted) {
    if (signal?.aborted) {
      emit(lines, '[AUTH_TIMEOUT]', 'Auth wait aborted by signal');
    } else {
      emit(lines, '[AUTH_TIMEOUT]', 'Auth wait timed out — user did not complete login');
      emit(lines, '[ACTION]', 'Complete login in the browser window, then re-run user-auth scheme or use scheme:"live-page" to inspect current state');
    }
  }

  return { evidenceLines: lines, details: { authCompleted } };
};

// ─── MVP Scheme: raw ──────────────────────────────────────────────────────────

// Domains that require an explicit .enable call before most methods work.
// CSS also needs DOM.enable first — handled below.
const DOMAINS_NEEDING_ENABLE = new Set([
  'DOM', 'CSS', 'Runtime', 'Network', 'Log', 'Page',
  'Performance', 'HeapProfiler', 'Profiler', 'Security',
  'ServiceWorker', 'Accessibility', 'DOMStorage', 'LayerTree',
  'Animation', 'Media', 'WebAudio', 'Audits', 'Preload', 'Overlay',
]);

const rawRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  const { method, sessionId } = params;
  // let so scriptFile/scriptSource overrides can mutate
  let rawParams: Record<string, unknown> = { ...(params.params ?? {}) };

  if (!method) {
    throw new Error('scheme:"raw" requires method:"Domain.method"');
  }

  // Auto-enable the domain before calling — many CDP methods silently fail
  // with -32600 if the domain isn't enabled first.
  const domain = method.split('.')[0] ?? '';
  if (DOMAINS_NEEDING_ENABLE.has(domain)) {
    if (domain === 'CSS') await session.send('DOM.enable', {}).catch(() => {});
    await session.send(`${domain}.enable` as never, {}).catch(() => {});
  }

  // scriptFile/scriptSource — avoids inline JSON escaping fragility for script injection.
  // Use scriptSource:"(function(){...})()" or scriptFile:"/abs/path/stealth-inject.mjs"
  // instead of nesting 10KB inside params:{source:"..."} which LLMs frequently mangle.
  if (params.scriptFile || params.scriptSource) {
    let src = params.scriptSource ?? '';
    if (!src && params.scriptFile) {
      // Bound the read to allowed roots — scriptFile content is injected into a
      // (possibly remote) page, so an unbounded read is a local-file exfil vector.
      // A blocked path throws visibly (not swallowed) so the caller learns why.
      assertPathAllowed(params.scriptFile, process.cwd(), 'scriptFile read');
      try {
        const txt = fs.readFileSync(params.scriptFile, 'utf8');
        const m = txt.match(/export const \w*SCRIPT\w*\s*=\s*`([\s\S]*?)`;/);
        src = m ? (m[1] ?? '') : txt;
      } catch { /* file not found — fall through */ }
    }
    if (src) {
      if (method === 'Page.addScriptToEvaluateOnNewDocument') rawParams = { ...rawParams, source: src };
      else if (method === 'Runtime.evaluate') rawParams = { ...rawParams, expression: src };
    }
  }

  emit(lines, '[RAW]', `calling ${method}`);

  let result: Record<string, unknown>;
  try {
    result = await session.send(method, rawParams, sessionId);
  } catch (err) {
    const e = err as Error;
    if (isCdpError(e)) {
      const marker = buildRetryMarker(e, method);
      for (const line of marker.split('\n')) lines.push(line);
      return { evidenceLines: lines, details: { error: e.message, method } };
    }
    throw err;
  }

  const resultStr = redactEvidence(JSON.stringify(result, null, 2));
  emit(lines, '[RAW_RESULT]', resultStr.slice(0, 2000));

  return { evidenceLines: lines, details: { method, result } };
};

// ─── Stub factory ─────────────────────────────────────────────────────────────


// ─── Stub implementations ─────────────────────────────────────────────────────

// Helper: evaluate JS expression that returns JSON
async function evalJson<T>(session: CdpSession, expr: string): Promise<T | null> {
  try {
    const r = await session.send('Runtime.evaluate', {
      expression: expr, returnByValue: true, awaitPromise: true,
    }) as { result?: { value?: unknown; type?: string } };
    const val = r?.result?.value;
    if (typeof val === 'string') { try { return JSON.parse(val) as T; } catch { return val as T; } }
    return (val ?? null) as T;
  } catch { return null; }
}

// ─── accessibility ─────────────────────────────────────────────────────────────

const accessibilityRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  if (params.url) await navigateAndWait(session, params.url);
  await session.send('DOM.enable', {});
  await session.send('Runtime.enable', {});
  await session.send('Accessibility.enable', {});

  const tree = await session.send('Accessibility.getFullAXTree', { depth: -1 }) as {
    nodes?: Array<{
      nodeId: string; ignored?: boolean; role?: { value: string }; name?: { value: string };
      properties?: Array<{ name: string; value: { value: unknown } }>; childIds?: string[];
    }>;
  };
  const nodes = (tree.nodes ?? []).filter(n => !n.ignored);
  emit(lines, '[A11Y]', `tree: ${nodes.length} visible nodes`);

  const findings: string[] = [];
  let lastHeadingLevel = 0;
  let hasMain = false;
  const roleCounts: Record<string, number> = {};

  for (const node of nodes) {
    const role = node.role?.value ?? '';
    const name = node.name?.value ?? '';
    roleCounts[role] = (roleCounts[role] ?? 0) + 1;
    if (role === 'RootWebArea' || role === 'main') hasMain = true;
    if (['button', 'link', 'textbox', 'combobox', 'checkbox', 'radio'].includes(role) && !name)
      findings.push(`[FINDING] AX_UNLABELED: role="${role}" has no accessible name`);
    if (role === 'img' && !name)
      findings.push('[FINDING] AX_MISSING_ALT: image has no accessible name');
    if (role === 'heading') {
      const lvlProp = node.properties?.find(p => p.name === 'level');
      const lvl = Number(lvlProp?.value?.value ?? 0);
      if (lvl > 0 && lastHeadingLevel > 0 && lvl > lastHeadingLevel + 1)
        findings.push(`[FINDING] AX_HEADING_SKIP: h${lastHeadingLevel} → h${lvl}`);
      if (lvl > 0) lastHeadingLevel = lvl;
    }
  }
  if (!hasMain) findings.push('[FINDING] AX_NO_MAIN_LANDMARK: no main landmark found');
  for (const f of findings) lines.push(f);

  const topRoles = Object.entries(roleCounts).sort((a,b)=>b[1]-a[1]).slice(0,8);
  emit(lines, '[A11Y]', `roles: ${topRoles.map(([r,c])=>`${r}:${c}`).join(', ')}`);

  // Quick DOM checks
  const checks = await evalJson<{imgs:number;inputs:number;emptyBtns:number;roles:number}>(session,
    'JSON.stringify({imgs:document.querySelectorAll("img:not([alt])").length,inputs:document.querySelectorAll("input:not([aria-label]):not([id]):not([title])").length,emptyBtns:document.querySelectorAll("button:empty").length,roles:document.querySelectorAll("[role]").length})');
  if (checks) {
    if (checks.imgs > 0) emit(lines, '[A11Y]', `imgs without alt: ${checks.imgs}`);
    if (checks.inputs > 0) emit(lines, '[A11Y]', `inputs without label: ${checks.inputs}`);
    if (checks.emptyBtns > 0) emit(lines, '[A11Y]', `empty buttons: ${checks.emptyBtns}`);
    emit(lines, '[A11Y]', `elements with [role]: ${checks.roles}`);
  }
  emit(lines, '[A11Y]', `findings: ${findings.length}`);
  return { evidenceLines: lines, details: { nodeCount: nodes.length, findings: findings.length } };
};

// ─── supply-chain ─────────────────────────────────────────────────────────────

const supplyChainRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  await session.send('Network.enable', {});
  await session.send('Runtime.enable', {});

  const externalScripts: Array<{ url: string; hasSri: boolean }> = [];
  const externalDomains = new Set<string>();

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Network.requestWillBeSent', (ev: unknown) => {
    const e = ev as { request?: { url?: string }; type?: string };
    const url = e.request?.url ?? '';
    if (e.type === 'Script' && url.startsWith('http')) {
      try {
        const host = new URL(url).hostname;
        if (params.url) {
          const pageHost = new URL(params.url).hostname;
          if (host !== pageHost && !host.endsWith(`.${pageHost.split('.').slice(-2).join('.')}`)) {
            externalDomains.add(host);
            externalScripts.push({ url, hasSri: false }); // SRI checked via DOM below
          }
        }
      } catch { /* ignore */ }
    }
  });

  if (params.url) await navigateAndWait(session, params.url, params.durationMs ?? 4000, signal);

  // DOM-based SRI check
  const scriptData = await evalJson<Array<{src:string;integrity:string|null;crossorigin:string|null}>>(session,
    'JSON.stringify([...document.querySelectorAll("script[src]")].map(s=>({src:s.src,integrity:s.integrity||null,crossorigin:s.crossOrigin||null})))');

  const domExternal: Array<{src:string;integrity:string|null}> = [];
  if (scriptData && params.url) {
    try {
      const pageHost = new URL(params.url).hostname;
      for (const s of scriptData) {
        try {
          const host = new URL(s.src).hostname;
          if (host !== pageHost && !host.endsWith(`.${pageHost.split('.').slice(-2).join('.')}`)) {
            domExternal.push(s);
            externalDomains.add(host);
          }
        } catch { /* ignore */ }
      }
    } catch { /* ignore */ }
  }

  emit(lines, '[SUPPLY]', `external domains: ${externalDomains.size}`);
  for (const domain of externalDomains) emit(lines, '[SUPPLY]', `  domain: ${domain}`);

  emit(lines, '[SUPPLY]', `external scripts (DOM): ${domExternal.length}`);
  for (const s of domExternal) {
    const sri = s.integrity ? `SRI:${s.integrity.slice(0,20)}…` : 'NO_SRI';
    emit(lines, '[SUPPLY]', `  ${sri} ${s.src.slice(0,100)}`);
    if (!s.integrity) lines.push(redactEvidence(`[FINDING] NO_SRI: ${s.src.slice(0,100)}`));
  }

  if (externalDomains.size > 10) lines.push(`[FINDING] HIGH_THIRD_PARTY_COUNT: ${externalDomains.size} external domains`);

  // Check for HTTP script loads
  for (const s of domExternal) {
    if (s.src.startsWith('http:')) emit(lines, '[FINDING]', `INSECURE_SCRIPT_LOAD: ${s.src.slice(0,100)}`);
  }

  const totalScripts = scriptData?.length ?? 0;
  emit(lines, '[SUPPLY]', `=== Summary: ${totalScripts} scripts total, ${externalDomains.size} external domains ===`);
  return { evidenceLines: lines, details: { externalDomains: externalDomains.size, externalScripts: domExternal.length } };
};

// ─── inject ───────────────────────────────────────────────────────────────────

const injectRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  await session.send('Page.enable', {});

  if (params.bypassCSP) {
    await session.send('Page.setBypassCSP', { enabled: true });
    emit(lines, '[INJECT]', 'CSP bypass enabled');
  }

  const source = params.scriptSource ?? params.expression ?? STEALTH_SCRIPT;
  const { identifier } = await session.send('Page.addScriptToEvaluateOnNewDocument', { source }) as { identifier: string };
  emit(lines, '[INJECT]', `script registered, identifier: ${identifier}`);
  emit(lines, '[INJECT]', `runs before any page JS on every navigation`);

  if (params.url) {
    await navigateAndWait(session, params.url);
    emit(lines, '[INJECT]', `navigated to ${params.url} — script executed`);
    // Verify injection by checking a side effect
    if (source.includes('navigator.webdriver')) {
      const wd = await evalJson<boolean|undefined>(session, 'navigator.webdriver');
      emit(lines, '[INJECT]', `navigator.webdriver = ${wd} (expected undefined)`);
    }
  }

  emit(lines, '[ACTION]', `Call Page.removeScriptToEvaluateOnNewDocument with identifier:${identifier} when done`);
  return { evidenceLines: lines, details: { identifier } };
};

// ─── scrape ───────────────────────────────────────────────────────────────────

const scrapeRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  if (params.url) await navigateAndWait(session, params.url);
  await session.send('DOM.enable', {});
  await session.send('Runtime.enable', {});

  const selector = params.selector ?? 'body';

  // CSS selector extraction
  const items = await evalJson<Array<Record<string,string>>>(session,
    `JSON.stringify([...document.querySelectorAll(${JSON.stringify(selector)})].slice(0,${params.depth ?? 50}).map(el=>{const t=el.tagName.toLowerCase();const r={tag:t};if(el.textContent)r.text=el.textContent.trim().slice(0,200);if(el.href)r.href=el.href;if(el.src)r.src=el.src;if(el.alt)r.alt=el.alt;if(el.value&&el.type!=='password'&&el.type!=='hidden')r.value=el.value;if(el.name)r.name=el.name;if(el.id)r.id=el.id;if(el.className)r.class=typeof el.className==='string'?el.className.trim().slice(0,60):'';return r}))`);

  emit(lines, '[SCRAPE]', `selector "${selector}": ${items?.length ?? 0} results`);
  for (const item of items ?? []) {
    const parts = Object.entries(item).filter(([,v])=>v).map(([k,v])=>`${k}="${String(v).slice(0,80)}"`).join(' ');
    emit(lines, '[SCRAPE]', `  ${parts}`);
  }

  // XPath support
  if (params.xpath) {
    const xpItems = await evalJson<Array<{tag:string;text:string}>>(session,
      `JSON.stringify((()=>{const it=document.evaluate(${JSON.stringify(params.xpath)},document,null,XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);const out=[];for(let i=0;i<Math.min(it.snapshotLength,50);i++){const n=it.snapshotItem(i);if(n?.nodeType===1)out.push({tag:n.tagName.toLowerCase(),text:n.textContent?.trim().slice(0,200)})}return out})())`);
    emit(lines, '[SCRAPE]', `xpath "${params.xpath}": ${xpItems?.length ?? 0} results`);
    for (const item of xpItems ?? []) emit(lines, '[SCRAPE]', `  <${item.tag}> "${item.text}"`);
  }

  emit(lines, '[SCRAPE]', `=== Summary: ${items?.length ?? 0} items extracted ===`);
  return { evidenceLines: lines, details: { count: items?.length ?? 0 } };
};

// ─── consent ──────────────────────────────────────────────────────────────────

const consentRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  await session.send('Network.enable', {});
  await session.send('Runtime.enable', {});

  const trackerHits: string[] = [];
  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Network.requestWillBeSent', (ev: unknown) => {
    const url = ((ev as { request?: { url?: string } }).request?.url) ?? '';
    if (/googletagmanager|google-analytics|clarity\.ms|bat\.bing|fbq|meta\.net|doubleclick|twitter\.com\/i\/adsct|ads-twitter/i.test(url))
      trackerHits.push(url.slice(0, 100));
  });

  if (params.url) await navigateAndWait(session, params.url, params.durationMs ?? 6000, signal);

  // CMP detection
  const cmp = await evalJson<Record<string,boolean>>(session,
    'JSON.stringify({usercentrics:!!(window.UC_UI||window.usercentrics),onetrust:!!(window.OneTrust||window.Optanon),cookiebot:!!(window.CookieConsent||window.Cookiebot),trustArc:!!window.truste,iabTCF:typeof window.__tcfapi==="function",gtmLoaded:Array.isArray(window.dataLayer),dlEvents:(window.dataLayer||[]).length})');

  if (cmp) {
    emit(lines, '[CONSENT]', `CMP: ${JSON.stringify(cmp)}`);
    if (!Object.values(cmp).slice(0,5).some(Boolean)) lines.push('[FINDING] NO_CMP: no consent management platform detected');
  }

  // Consent state
  const cs = await evalJson<Record<string,unknown>>(session,
    'JSON.stringify({ucGcm:(()=>{try{return JSON.parse(localStorage.getItem("ucData")||"{}").gcm||null}catch{return null}})(),ucString:!!localStorage.getItem("ucString"),iabConsent:!!localStorage.getItem("eupubconsent-v2"),otConsent:!!localStorage.getItem("OptanonConsent")})');
  if (cs?.ucGcm && typeof cs.ucGcm === 'object') {
    const gcm = cs.ucGcm as Record<string,string>;
    emit(lines, '[CONSENT]', `GCM flags: ${JSON.stringify(gcm)}`);
    if (gcm['adStorage']==='granted') lines.push('[FINDING] CONSENT_PRE_GRANTED: adStorage=granted');
    if (gcm['analyticsStorage']==='granted') lines.push('[FINDING] CONSENT_PRE_GRANTED: analyticsStorage=granted');
  }
  if (cs?.iabConsent) emit(lines, '[CONSENT]', 'IAB TCF v2 consent string present');
  if (cs?.otConsent) emit(lines, '[CONSENT]', 'OneTrust consent string present');

  // Tracker pre-grant
  emit(lines, '[CONSENT]', `trackers on cold load: ${trackerHits.length}`);
  for (const h of trackerHits.slice(0, 10)) emit(lines, '[CONSENT]', `  tracker: ${h}`);
  if (trackerHits.length > 0) lines.push(`[FINDING] TRACKERS_BEFORE_CONSENT: ${trackerHits.length} tracker requests fired before consent`);

  // dataLayer events
  const dl = await evalJson<string[]>(session,
    'JSON.stringify((window.dataLayer||[]).slice(0,10).map(e=>e.event||JSON.stringify(e).slice(0,60)))');
  if (dl?.length) emit(lines, '[CONSENT]', `dataLayer events: ${dl.join(', ')}`);

  emit(lines, '[CONSENT]', `=== Summary: CMP=${Object.values(cmp??{}).slice(0,5).some(Boolean)}, trackers=${trackerHits.length} ===`);
  return { evidenceLines: lines, details: { trackerCount: trackerHits.length } };
};

// ─── emulate ──────────────────────────────────────────────────────────────────

const DEVICE_PRESETS: Record<string, {width:number;height:number;dpr:number;mobile:boolean;ua:string}> = {
  iphone15: {width:393,height:852,dpr:3,mobile:true,ua:'Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1'},
  iphone13: {width:390,height:844,dpr:3,mobile:true,ua:'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1'},
  pixel7: {width:412,height:915,dpr:2.625,mobile:true,ua:'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Mobile Safari/537.36'},
  'galaxy-s23': {width:360,height:780,dpr:3,mobile:true,ua:'Mozilla/5.0 (Linux; Android 13; SM-S911B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Mobile Safari/537.36'},
  'ipad-air': {width:820,height:1180,dpr:2,mobile:true,ua:'Mozilla/5.0 (iPad; CPU OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1'},
  'desktop-hd': {width:1920,height:1080,dpr:1,mobile:false,ua:''},
};

const THROTTLE_PRESETS: Record<string,{offline:boolean;downloadThroughput:number;uploadThroughput:number;latency:number}> = {
  slow3g: {offline:false,downloadThroughput:50_000,uploadThroughput:20_000,latency:400},
  fast3g: {offline:false,downloadThroughput:180_000,uploadThroughput:84_000,latency:100},
  offline: {offline:true,downloadThroughput:0,uploadThroughput:0,latency:0},
  reset: {offline:false,downloadThroughput:-1,uploadThroughput:-1,latency:0},
};

const emulateRecipe: Recipe = async ({ session, params, screenshotDir }) => {
  const lines: string[] = [];
  await session.send('Page.enable', {});
  await session.send('Network.enable', {});

  const d = params.device;
  const presetKey = (d as unknown as {preset?: string})?.preset ?? '';
  const preset = DEVICE_PRESETS[presetKey];

  if (d || preset) {
    const width = d?.width ?? preset?.width ?? 1280;
    const height = d?.height ?? preset?.height ?? 800;
    const dpr = d?.deviceScaleFactor ?? preset?.dpr ?? 1;
    const mobile = d?.mobile ?? preset?.mobile ?? false;
    await session.send('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: dpr, mobile });
    emit(lines, '[EMULATE]', `device: ${width}x${height} dpr=${dpr} mobile=${mobile}`);
    if (mobile) {
      await session.send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 });
      emit(lines, '[EMULATE]', 'touch enabled');
    }
    const ua = d?.userAgent ?? preset?.ua;
    if (ua) {
      await session.send('Emulation.setUserAgentOverride', { userAgent: ua });
      emit(lines, '[EMULATE]', `UA: ${ua.slice(0, 80)}`);
    }
  }

  const throttleKey = (params as unknown as { throttlePreset?: string }).throttlePreset;
  const throttleDirect = params.throttle;
  const throttle = throttleKey ? THROTTLE_PRESETS[throttleKey] : throttleDirect;
  if (throttle) {
    await session.send('Network.emulateNetworkConditions', {
      offline: throttle.offline ?? false,
      downloadThroughput: throttle.downloadThroughput ?? -1,
      uploadThroughput: throttle.uploadThroughput ?? -1,
      latency: throttle.latency ?? 0,
    });
    emit(lines, '[EMULATE]', `throttle: down=${throttle.downloadThroughput} up=${throttle.uploadThroughput} latency=${throttle.latency}ms offline=${throttle.offline}`);
  }

  if (params.url) {
    await navigateAndWait(session, params.url);
    emit(lines, '[EMULATE]', `navigated to ${params.url}`);
    const v = await evalJson<{width:number;dpr:number;mobile:boolean}>(session,
      'JSON.stringify({width:window.innerWidth,dpr:devicePixelRatio,mobile:/Mobi/.test(navigator.userAgent)})');
    if (v) emit(lines, '[EMULATE]', `verified: innerWidth=${v.width} dpr=${v.dpr} mobileUA=${v.mobile}`);
    const { path: ssPath } = await captureScreenshot(session, { screenshotDir, scheme: 'emulate', targetUrl: params.url });
    emit(lines, '[SCREENSHOT]', ssPath);
  }
  return { evidenceLines: lines, details: {} };
};

// ─── monitor ──────────────────────────────────────────────────────────────────

const monitorRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const durationMs = params.durationMs ?? 30_000;
  const intervalMs = 5_000;
  await session.send('Network.enable', {});
  await session.send('Runtime.enable', {});
  await session.send('Log.enable', {});

  const queue: string[] = [];
  let prevUrl = '';

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Runtime.exceptionThrown', (ev: unknown) => {
    const e = ev as { exceptionDetails?: { text?: string } };
    queue.push(`exception: ${e.exceptionDetails?.text ?? 'unknown'}`);
  });
  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Network.responseReceived', (ev: unknown) => {
    const e = ev as { response?: { status?: number; url?: string } };
    const status = e.response?.status ?? 0;
    if (status >= 400) queue.push(`HTTP ${status} ${e.response?.url?.slice(0,80)}`);
  });

  if (params.url) await navigateAndWait(session, params.url, Math.min(4000, durationMs / 2), signal);

  emit(lines, '[MONITOR]', `watching for ${durationMs}ms at ${intervalMs}ms intervals`);
  const endTime = Date.now() + durationMs;
  let iteration = 0;

  while (Date.now() < endTime && !signal?.aborted) {
    const snapshot = await evalJson<{url:string;title:string;errorEls:number}>(session,
      'JSON.stringify({url:location.href,title:document.title,errorEls:document.querySelectorAll(".error,[aria-invalid=\"true\"],[data-error]").length})');
    if (snapshot) {
      emit(lines, '[MONITOR]', `t=${iteration*intervalMs/1000}s url=${snapshot.url.slice(0,60)} title="${snapshot.title.slice(0,40)}" errorEls=${snapshot.errorEls}`);
      if (prevUrl && prevUrl !== snapshot.url) emit(lines, '[FINDING]', `MONITOR_REDIRECT: URL changed to ${snapshot.url}`);
      if (snapshot.errorEls > 0) lines.push(`[FINDING] MONITOR_DOM_ERROR: ${snapshot.errorEls} error elements at t=${iteration*intervalMs/1000}s`);
      prevUrl = snapshot.url;
    }
    while (queue.length) lines.push(`[FINDING] MONITOR_EVENT: ${redactEvidence(queue.shift()!)}`);
    iteration++;
    await new Promise(r => setTimeout(r, Math.min(intervalMs, endTime - Date.now())));
  }
  emit(lines, '[MONITOR]', `=== completed ${iteration} iterations ===`);
  return { evidenceLines: lines, details: { iterations: iteration, durationMs } };
};

// ─── workers ──────────────────────────────────────────────────────────────────

const workersRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const durationMs = params.durationMs ?? 6000;

  // Must enable BEFORE navigation
  await session.send('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: false, flatten: true });
  await session.send('Target.setDiscoverTargets', { discover: true });
  await session.send('ServiceWorker.enable', {});

  const workerSessions: Array<{type:string;url:string;sessionId:string}> = [];

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Target.attachedToTarget', async (ev: unknown) => {
    const e = ev as { targetInfo?: { type?: string; url?: string }; sessionId?: string };
    const type = e.targetInfo?.type ?? '';
    const url = e.targetInfo?.url ?? '';
    const sessionId = e.sessionId ?? '';
    if (!['worker','shared_worker','service_worker'].includes(type)) return;
    workerSessions.push({ type, url, sessionId });
    emit(lines, '[WORKER]', `attached: type=${type} url=${url || '(blob)'}`);
    if (type === 'worker' && !url) lines.push('[FINDING] WORKER_BLOB: blob worker (opaque)');
    if (type === 'shared_worker') lines.push(`[FINDING] SHARED_WORKER: ${url}`);
    if (type === 'service_worker') lines.push(`[FINDING] SERVICE_WORKER_TARGET: ${url}`);
  });

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('ServiceWorker.workerVersionUpdated', (ev: unknown) => {
    const e = ev as { versions?: Array<{ scriptURL?: string; status?: string; runningStatus?: string }> };
    for (const v of e.versions ?? []) {
      emit(lines, '[SW]', `version: status=${v.status} running=${v.runningStatus} script=${v.scriptURL}`);
      if (v.status === 'activated') emit(lines, '[FINDING]', `SW_ACTIVATED: ${v.scriptURL}`);
    }
  });

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('ServiceWorker.workerRegistrationUpdated', (ev: unknown) => {
    const e = ev as { registrations?: Array<{ scopeURL?: string; isDeleted?: boolean }> };
    for (const r of e.registrations ?? []) {
      emit(lines, '[SW]', `registered: scope=${r.scopeURL} deleted=${r.isDeleted}`);
      if (!r.isDeleted) lines.push(`[FINDING] SW_REGISTERED: ${r.scopeURL}`);
    }
  });

  // Existing targets
  const targets = await session.send('Target.getTargets', {}) as { targetInfos?: Array<{type:string;url:string}> };
  const existing = (targets.targetInfos ?? []).filter(t => ['worker','shared_worker','service_worker'].includes(t.type));
  emit(lines, '[WORKER]', `existing worker targets: ${existing.length}`);
  for (const t of existing) emit(lines, '[WORKER]', `  ${t.type}: ${t.url || '(blob)'}`);

  if (params.url) {
    await session.send('Page.enable', {});
    await navigateAndWait(session, params.url, Math.min(4000, durationMs), signal);
  }
  await new Promise(r => setTimeout(r, Math.min(Math.max(durationMs - 4000, 0), 2000)));

  // SW snapshot via JS
  const swSnap = await evalJson<Array<{scope:string;state:string;script:string}>>(session,
    'navigator.serviceWorker?navigator.serviceWorker.getRegistrations().then(r=>JSON.stringify(r.map(s=>({scope:s.scope,state:(s.active||s.installing||s.waiting)?.state||"none",script:(s.active||s.installing||s.waiting)?.scriptURL})))):JSON.stringify([])');
  if (swSnap?.length) {
    emit(lines, '[SW]', `runtime snapshot: ${swSnap.length} registration(s)`);
    for (const sw of swSnap) emit(lines, '[SW]', `  scope=${sw.scope} state=${sw.state}`);
  }

  emit(lines, '[WORKER]', `=== Summary: ${workerSessions.length} workers attached ===`);
  await session.send('Target.setAutoAttach', { autoAttach: false, waitForDebuggerOnStart: false, flatten: true }).catch(()=>{});
  await session.send('ServiceWorker.disable', {}).catch(()=>{});
  return { evidenceLines: lines, details: { workerCount: workerSessions.length } };
};

// ─── service-worker ───────────────────────────────────────────────────────────

const serviceWorkerRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const durationMs = params.durationMs ?? 6000;
  await session.send('ServiceWorker.enable', {});

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('ServiceWorker.workerRegistrationUpdated', (ev: unknown) => {
    const e = ev as { registrations?: Array<{ registrationId?: string; scopeURL?: string; isDeleted?: boolean }> };
    for (const r of e.registrations ?? []) {
      if (!r.isDeleted) { emit(lines, '[SW]', `registered: scope=${r.scopeURL}`); lines.push(`[FINDING] SW_REGISTERED: ${r.scopeURL}`); }
      else lines.push(`[FINDING] SW_REMOVED: ${r.scopeURL}`);
    }
  });

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('ServiceWorker.workerVersionUpdated', (ev: unknown) => {
    const e = ev as { versions?: Array<{ scriptURL?: string; status?: string; runningStatus?: string }> };
    for (const v of e.versions ?? []) {
      emit(lines, '[SW]', `version: ${v.status}/${v.runningStatus} script=${v.scriptURL}`);
      if (v.status === 'activated') emit(lines, '[FINDING]', `SW_ACTIVATED: ${v.scriptURL}`);
      const host = (() => { try { return new URL(v.scriptURL ?? '').hostname; } catch { return ''; } })();
      const pageHost = (() => { try { return new URL(params.url ?? 'about:blank').hostname; } catch { return ''; } })();
      if (host && pageHost && host !== pageHost) emit(lines, '[FINDING]', `SW_THIRD_PARTY_SCRIPT: ${v.scriptURL}`);
    }
  });

  if (params.url) await navigateAndWait(session, params.url, Math.min(4000, durationMs), signal);
  await new Promise(r => setTimeout(r, Math.min(Math.max(durationMs - 4000, 0), 2000)));

  // Point-in-time snapshot
  const regs = await evalJson<Array<{scope:string;state:string;script:string}>>(session,
    'navigator.serviceWorker?navigator.serviceWorker.getRegistrations().then(r=>JSON.stringify(r.map(x=>({scope:x.scope,state:(x.active||x.installing||x.waiting)?.state||"none",script:(x.active||x.installing||x.waiting)?.scriptURL})))):JSON.stringify([])');
  if (regs !== null) {
    emit(lines, '[SW]', `snapshot: ${(regs as unknown[]).length} registration(s)`);
    for (const r of regs ?? []) emit(lines, '[SW]', `  scope=${r.scope} state=${r.state} script=${r.script}`);
  }
  await session.send('ServiceWorker.disable', {}).catch(()=>{});
  return { evidenceLines: lines, details: { registrations: regs?.length ?? 0 } };
};

// ─── websocket ────────────────────────────────────────────────────────────────

const websocketRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const durationMs = params.durationMs ?? 10_000;
  await session.send('Network.enable', {});

  const wsMap = new Map<string, { url: string; frames: number }>();

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Network.webSocketCreated', (ev: unknown) => {
    const e = ev as { requestId?: string; url?: string };
    if (e.requestId && e.url) { wsMap.set(e.requestId, { url: e.url, frames: 0 }); emit(lines, '[WS]', `created: ${e.url}`); }
    if (e.url) {
      const host = (() => { try { return new URL(e.url!).hostname; } catch { return e.url; } })();
      if (params.url && (() => { try { return new URL(params.url!).hostname !== host; } catch { return false; } })())
        lines.push(`[FINDING] WS_UNKNOWN_HOST: ${e.url}`);
    }
  });

  for (const ev of ['Network.webSocketFrameSent','Network.webSocketFrameReceived']) {
    (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on(ev, (e: unknown) => {
      const evt = e as { requestId?: string; response?: { payloadData?: string; opcode?: number } };
      const ws = wsMap.get(evt.requestId ?? '');
      if (ws) ws.frames++;
      const payload = evt.response?.payloadData ?? '';
      if (/token|password|key|secret/i.test(payload)) lines.push('[FINDING] SENSITIVE_IN_WS_FRAME: sensitive data detected');
      if (payload.length > 102400) lines.push(`[FINDING] LARGE_WS_FRAME: ${Math.round(payload.length/1024)}KB`);
    });
  }

  (session as unknown as { on(ev:string,fn:(p:unknown)=>void):void }).on('Network.webSocketClosed', (ev: unknown) => {
    const e = ev as { requestId?: string };
    const ws = wsMap.get(e.requestId ?? '');
    if (ws) emit(lines, '[WS]', `closed: ${ws.url} frames=${ws.frames}`);
  });

  if (params.url) await navigateAndWait(session, params.url, Math.min(4000, durationMs), signal);
  await new Promise(r => setTimeout(r, Math.min(Math.max(durationMs - 4000, 0), 2000)));

  emit(lines, '[WS]', `=== Summary: ${wsMap.size} WebSocket connection(s) ===`);
  for (const [,ws] of wsMap) emit(lines, '[WS]', `  ${ws.url} total_frames=${ws.frames}`);
  return { evidenceLines: lines, details: { wsCount: wsMap.size } };
};

// ─── memory ───────────────────────────────────────────────────────────────────

const memoryRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  if (params.url) await navigateAndWait(session, params.url);
  await session.send('Performance.enable', {});

  const dom = await session.send('Memory.getDOMCounters', {}) as { documents?: number; nodes?: number; jsEventListeners?: number };
  emit(lines, '[MEMORY]', `DOM: documents=${dom.documents ?? 0} nodes=${dom.nodes ?? 0} listeners=${dom.jsEventListeners ?? 0}`);
  if ((dom.nodes ?? 0) > 10_000) lines.push(`[FINDING] LARGE_DOM: ${dom.nodes} nodes`);
  if ((dom.jsEventListeners ?? 0) > 5_000) lines.push(`[FINDING] HIGH_LISTENER_COUNT: ${dom.jsEventListeners}`);

  const perf = await session.send('Performance.getMetrics', {}) as { metrics?: Array<{name:string;value:number}> };
  const m = (perf.metrics ?? []).reduce<Record<string,number>>((a,{name,value})=>(a[name]=value,a), {});
  const heapUsed = m['JSHeapUsedSize'] ?? 0;
  const heapTotal = m['JSHeapTotalSize'] ?? 0;
  const pct = heapTotal > 0 ? ((heapUsed/heapTotal)*100).toFixed(1) : '0';
  emit(lines, '[MEMORY]', `jsHeap: ${Math.round(heapUsed/1024/1024)}MB / ${Math.round(heapTotal/1024/1024)}MB (${pct}%)`);
  emit(lines, '[MEMORY]', `layout: ${m['LayoutCount'] ?? 0} recalcStyle: ${m['RecalcStyleCount'] ?? 0} scriptDuration: ${((m['ScriptDuration'] ?? 0)*1000).toFixed(0)}ms`);
  if (heapTotal > 0 && heapUsed/heapTotal > 0.9) lines.push(`[FINDING] HEAP_PRESSURE: ${pct}% heap used`);
  return { evidenceLines: lines, details: { domNodes: dom.nodes ?? 0, heapMB: Math.round(heapUsed/1024/1024) } };
};

// ─── css-coverage ─────────────────────────────────────────────────────────────

const cssCoverageRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  await session.send('DOM.enable', {});
  await session.send('CSS.enable', {});
  await session.send('CSS.startRuleUsageTracking', {});
  if (params.url) await navigateAndWait(session, params.url, params.durationMs ?? 4000);
  const result = await session.send('CSS.stopRuleUsageTracking', {}) as {
    ruleUsage?: Array<{ styleSheetId: string; used: boolean }>;
  };
  const rules = result.ruleUsage ?? [];
  const used = rules.filter(r => r.used).length;
  const pct = rules.length > 0 ? ((used/rules.length)*100).toFixed(1) : '0';
  emit(lines, '[COVERAGE]', `CSS: ${used}/${rules.length} rules used (${pct}%)`);

  const bySheet = rules.reduce<Record<string,{used:number;total:number}>>((a,r) => {
    if (!a[r.styleSheetId]) a[r.styleSheetId] = {used:0,total:0};
    a[r.styleSheetId]!.total++;
    if (r.used) a[r.styleSheetId]!.used++;
    return a;
  }, {});
  for (const [id,s] of Object.entries(bySheet).slice(0,5))
    emit(lines, '[COVERAGE]', `  sheet ${id.slice(0,12)}: ${s.used}/${s.total} (${s.total>0?((s.used/s.total)*100).toFixed(0):0}%)`);
  emit(lines, '[COVERAGE]', `=== CSS coverage: ${pct}% of ${rules.length} rules used ===`);
  return { evidenceLines: lines, details: { cssUsed: used, cssTotal: rules.length } };
};

// ─── js-coverage ──────────────────────────────────────────────────────────────

const jsCoverageRecipe: Recipe = async ({ session, params }) => {
  const lines: string[] = [];
  await session.send('Profiler.enable', {});
  await session.send('Profiler.startPreciseCoverage', { callCount: false, detailed: true });
  if (params.url) await navigateAndWait(session, params.url, params.durationMs ?? 4000);
  const result = await session.send('Profiler.takePreciseCoverage', {}) as {
    result?: Array<{ scriptId: string; url: string; functions: Array<{ ranges: Array<{ startOffset: number; endOffset: number; count: number }> }> }>;
  };
  await session.send('Profiler.stopPreciseCoverage', {});

  const scripts = (result.result ?? []).filter(s => s.url && !s.url.startsWith('extensions::'));
  let totalBytes = 0, coveredBytes = 0;
  const byScript: Array<{url:string;pct:number;unused:number}> = [];

  for (const script of scripts) {
    let t = 0, c = 0;
    for (const fn of script.functions) for (const r of fn.ranges) {
      const bytes = r.endOffset - r.startOffset;
      t += bytes; if (r.count > 0) c += bytes;
    }
    totalBytes += t; coveredBytes += c;
    if (t > 0) byScript.push({ url: script.url, pct: (c/t)*100, unused: t-c });
  }

  const pct = totalBytes > 0 ? ((coveredBytes/totalBytes)*100).toFixed(1) : '0';
  emit(lines, '[COVERAGE]', `JS: ${coveredBytes}B/${totalBytes}B covered (${pct}%) across ${scripts.length} scripts`);
  for (const s of byScript.sort((a,b)=>a.pct-b.pct).slice(0,8))
    emit(lines, '[COVERAGE]', `  ${s.pct.toFixed(0)}% ${s.url.slice(-60)} (${Math.round(s.unused/1024)}KB unused)`);
  emit(lines, '[COVERAGE]', `=== JS coverage: ${pct}% of ${Math.round(totalBytes/1024)}KB used ===`);
  return { evidenceLines: lines, details: { jsCovered: coveredBytes, jsTotal: totalBytes } };
};

// ─── login ────────────────────────────────────────────────────────────────────

const loginRecipe: Recipe = async ({ session, params, signal }) => {
  const lines: string[] = [];
  const timeoutMs = params.timeoutMs ?? 120_000;
  await session.send('Network.enable', {});
  await session.send('Page.enable', {});

  if (params.url) await navigateAndWait(session, params.url, 5000, signal);
  emit(lines, '[AUTH]', `waiting for user to complete login (timeout: ${Math.round(timeoutMs/1000)}s)`);
  emit(lines, '[AUTH]', `current URL: ${((await session.send('Runtime.evaluate', { expression: 'location.href', returnByValue: true }) as { result?: { value?: string } }).result?.value) ?? '?'}`);
  emit(lines, '[ACTION]', 'Complete login in the browser window then run scheme:"live-page" or scheme:"storage" to inspect authenticated state');

  // Watch for auth completion
  const endTime = Date.now() + timeoutMs;
  let authCompleted = false;
  const startUrl = params.url ?? '';

  while (Date.now() < endTime && !signal?.aborted) {
    await new Promise(r => setTimeout(r, 2000));
    const currentUrl = ((await session.send('Runtime.evaluate', { expression: 'location.href', returnByValue: true }).catch(()=>({result:{value:''}}))) as { result?: { value?: string } }).result?.value ?? '';
    if (currentUrl && startUrl && currentUrl !== startUrl && !currentUrl.includes('/login') && !currentUrl.includes('/signin')) {
      authCompleted = true;
      emit(lines, '[AUTH_COMPLETE]', `user authenticated — now at ${currentUrl}`);
      emit(lines, '[ACTION]', 'Auth complete — run scheme:"security" or scheme:"storage" to inspect auth state');
      break;
    }
  }
  if (!authCompleted) emit(lines, '[AUTH_TIMEOUT]', 'Auth wait timed out — user did not complete login');
  return { evidenceLines: lines, details: { authCompleted } };
};

// ─── full-audit ───────────────────────────────────────────────────────────────

const fullAuditRecipe: Recipe = async ({ session, params, screenshotDir, signal }) => {
  const lines: string[] = [];
  emit(lines, '[AUDIT]', `starting full audit of ${params.url ?? '(current tab)'}`);

  // Navigate once, then run all schemes without re-navigating
  if (params.url) await navigateAndWait(session, params.url, params.durationMs ?? 5000, signal);
  const p0 = { ...params, url: undefined };

  const runScheme = async (label: string, recipe: Recipe) => {
    try {
      emit(lines, '[AUDIT]', `=== ${label} ===`);
      const r = await recipe({ session, params: p0, screenshotDir, signal });
      lines.push(...r.evidenceLines);
    } catch (err) {
      emit(lines, '[AUDIT]', `${label} error: ${(err as Error).message.slice(0,80)}`);
    }
  };

  await runScheme('network', networkRecipe);
  await runScheme('console', consoleRecipe);
  await runScheme('security', securityRecipe);
  await runScheme('storage', storageRecipe);
  await runScheme('accessibility', accessibilityRecipe);
  await runScheme('supply-chain', supplyChainRecipe);
  await runScheme('memory', memoryRecipe);

  // Screenshot at end
  const { path: ssPath } = await captureScreenshot(session, { screenshotDir, scheme: 'full-audit', targetUrl: params.url });
  emit(lines, '[SCREENSHOT]', ssPath);

  const findings = lines.filter(l => l.includes('[FINDING]')).length;
  emit(lines, '[AUDIT]', `=== COMPLETE: ${findings} findings total ===`);
  return { evidenceLines: lines, details: { findings } };
};

// All 15 stub schemes now have full implementations above.

// ─── SCHEME_REGISTRY ──────────────────────────────────────────────────────────

export const SCHEME_REGISTRY: Record<Scheme, SchemeEntry> = {
  debug: {
    domains: ['Network.enable', 'Runtime.enable', 'Log.enable', 'Page.enable', 'DOM.enable'],
    prefixes: ['[DEBUG]', '[EXCEPTION]', '[NETWORK_ERROR]', '[FINDING]', '[ACTION]'],
    recipe: debugRecipe,
  },
  network: {
    domains: ['Network.enable'],
    prefixes: ['[NETWORK]', '[NETWORK_FAILED]', '[FINDING]', '[ACTION]'],
    recipe: networkRecipe,
  },
  console: {
    domains: ['Runtime.enable', 'Log.enable'],
    prefixes: ['[CONSOLE:ERROR]', '[EXCEPTION]', '[FINDING]', '[ACTION]'],
    recipe: consoleRecipe,
  },
  dom: {
    domains: ['DOM.enable', 'Runtime.enable'],
    prefixes: ['[DOM]', '[FINDING]', '[ACTION]'],
    recipe: domRecipe,
  },
  performance: {
    domains: ['Performance.enable', 'Runtime.enable'],
    prefixes: ['[METRIC]', '[FINDING]', '[ACTION]'],
    recipe: performanceRecipe,
  },
  screenshot: {
    domains: ['Page.enable'],
    prefixes: ['[SCREENSHOT]', '[SCREENSHOT_ERROR]'],
    recipe: screenshotRecipe,
  },
  intercept: {
    domains: ['Network.enable', 'Fetch.enable'],
    prefixes: ['[INTERCEPT]', '[FINDING]', '[ACTION]'],
    recipe: interceptRecipe,
  },
  security: {
    domains: ['Network.enable', 'Security.enable'],
    prefixes: ['[SECURITY]', '[COOKIE]', '[FINDING]', '[ACTION]'],
    recipe: securityRecipe,
  },
  storage: {
    domains: ['Network.enable'],
    prefixes: ['[STORAGE]', '[FINDING]', '[ACTION]'],
    recipe: storageRecipe,
  },
  automate: {
    domains: ['Page.enable', 'Runtime.enable'],
    prefixes: ['[AUTOMATE]', '[ACTION]'],
    recipe: automateRecipe,
  },
  'live-page': {
    domains: ['Runtime.enable'],
    prefixes: ['[FINDING]', '[DOM]', '[EVAL]', '[ACTION]'],
    recipe: livePageRecipe,
  },
  'user-auth': {
    domains: ['Page.enable', 'Network.enable'],
    prefixes: ['[AUTH]', '[AUTH_COMPLETE]', '[AUTH_TIMEOUT]', '[ACTION]'],
    recipe: userAuthRecipe,
  },
  raw: {
    domains: [],
    prefixes: ['[RAW]', '[RAW_RESULT]', '[CDP_RETRY_NEEDED]'],
    recipe: rawRecipe,
  },

  // ── Stubs ──────────────────────────────────────────────────────────────────
  memory: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: memoryRecipe,
  },
  'css-coverage': {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: cssCoverageRecipe,
  },
  'js-coverage': {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: jsCoverageRecipe,
  },
  websocket: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: websocketRecipe,
  },
  'service-worker': {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: serviceWorkerRecipe,
  },
  workers: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: workersRecipe,
  },
  accessibility: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: accessibilityRecipe,
  },
  'supply-chain': {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: supplyChainRecipe,
  },
  'full-audit': {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: fullAuditRecipe,
  },
  consent: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: consentRecipe,
  },
  scrape: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: scrapeRecipe,
  },
  login: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: loginRecipe,
  },
  emulate: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: emulateRecipe,
  },
  inject: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: injectRecipe,
  },
  monitor: {
    domains: [],
    prefixes: ['[ACTION]'],
    recipe: monitorRecipe,
  },
};
