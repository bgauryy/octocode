/**
 * Chrome DevTools Protocol (CDP) engine for the chromeDebug Pi tool.
 *
 * Ports the proven transport, target selection, session reuse, and sandbox from
 * `_skills/octocode-chrome-devtools/scripts/cdp-runner.mjs` as an in-process
 * TypeScript module with AbortSignal cancellation, Chrome 136-safe launch/attach,
 * secret redaction, and a deterministic screenshot writer.
 *
 * Node 22+ native WebSocket — no `ws` dependency.
 */

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import { execSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import { getOctocodeHome } from './env.js';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface CdpTargetInfo {
  id: string;
  type: string;
  url: string;
  title: string;
  webSocketDebuggerUrl?: string;
  devtoolsFrontendUrl?: string;
}

export interface CdpSendOptions {
  sessionId?: string;
  timeoutMs?: number;
}

export interface CdpSession {
  targetInfo: CdpTargetInfo;
  /** Send a CDP method and receive the result. */
  send(method: string, params?: Record<string, unknown>, sessionId?: string): Promise<Record<string, unknown>>;
  /** Subscribe to CDP events. Use '*' to receive all events. */
  on(event: string, handler: (params: Record<string, unknown>, meta: { sessionId?: string }) => void): void;
  /** Unsubscribe a handler. */
  off(event: string, handler: (params: Record<string, unknown>, meta: { sessionId?: string }) => void): void;
  /** Close the WebSocket session. */
  close(): void;
  /** True if the session is closed. */
  readonly closed: boolean;
}

export interface CdpVersionInfo {
  Browser: string;
  'Protocol-Version': string;
  'User-Agent': string;
  'V8-Version'?: string;
  webSocketDebuggerUrl?: string;
}

export interface SessionIdentity {
  mode: 'attached' | 'launched';
  userDataDir?: string;
  browser: string;
  userAgent?: string;
  tabHost?: string;
  tabPath?: string;
  cookieNames?: string[];
}

export interface SessionMetadata {
  port: number;
  browser: string;
  mode: 'attached' | 'launched';
  userDataDir?: string;
  lastConnectedAt: string;
  activeTarget?: {
    id: string;
    type: string;
    url: string;
    title: string;
    via: string;
  };
  identity?: SessionIdentity;
}

export interface ChromeDebugConnectOptions {
  port?: number;
  targetId?: string;
  targetUrl?: string;
  targetType?: string;
  newTab?: string;
  browserUrl?: string;
  wsEndpoint?: string;
  userDataDir?: string;
  launch?: boolean;
  headless?: boolean;
  timeoutMs?: number;
  signal?: AbortSignal;
  workspaceCwd?: string;
}

export interface ChromeConnection {
  session: CdpSession;
  version: CdpVersionInfo;
  metadata: SessionMetadata;
  screenshotDir: string;
  sessionFile: string;
}

export interface EvidenceLine {
  prefix: string;
  text: string;
}

// ─── Localhost sandbox ────────────────────────────────────────────────────────

/** True when the URL hostname is localhost/127.0.0.1/::1 */
export function isLocalhost(url: string): boolean {
  try {
    const h = new URL(String(url)).hostname;
    return h === 'localhost' || h === '127.0.0.1' || h === '::1' || h === '[::1]';
  } catch {
    return false;
  }
}

const _origFetch = globalThis.fetch;

/** Patched fetch that only allows localhost URLs. */
export function restrictedFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url =
    typeof input === 'string' ? input
    : input instanceof URL ? input.href
    : (input as Request).url ?? '';
  if (!isLocalhost(url)) {
    throw new Error(`[SANDBOX] fetch blocked: only localhost allowed (attempted: ${url})`);
  }
  return _origFetch(input as RequestInfo, init);
}

// ─── CDP HTTP discovery ───────────────────────────────────────────────────────

async function cdpHttp<T = Record<string, unknown>>(
  port: number,
  urlPath: string,
  method = 'GET',
): Promise<T> {
  const url = `http://127.0.0.1:${port}${urlPath}`;
  if (!isLocalhost(url)) throw new Error(`[SANDBOX] CDP HTTP blocked: ${url}`);
  const res = await _origFetch(url, { method, signal: AbortSignal.timeout(5000) });
  if (!res.ok) throw new Error(`CDP HTTP ${res.status} for ${urlPath}`);
  return res.json() as Promise<T>;
}

export async function getVersion(port: number): Promise<CdpVersionInfo> {
  return cdpHttp<CdpVersionInfo>(port, '/json/version');
}

export async function getTargets(port: number): Promise<CdpTargetInfo[]> {
  return cdpHttp<CdpTargetInfo[]>(port, '/json');
}

async function openTab(port: number, url: string): Promise<CdpTargetInfo> {
  return cdpHttp<CdpTargetInfo>(port, `/json/new?${encodeURIComponent(url)}`, 'PUT');
}

async function activateTarget(port: number, id: string): Promise<void> {
  await cdpHttp(port, `/json/activate/${id}`).catch(() => undefined);
}

async function closeTab(port: number, id: string): Promise<boolean> {
  try {
    const res = await _origFetch(`http://127.0.0.1:${port}/json/close/${id}`, {
      signal: AbortSignal.timeout(3000),
    });
    return res.ok;
  } catch {
    return false;
  }
}

// ─── createCdpSession ─────────────────────────────────────────────────────────

/**
 * Opens a native WebSocket CDP session (Node 22+). Ported from cdp-runner.mjs.
 * Adds AbortSignal cancellation and per-call timeoutMs override.
 */
export function createCdpSession(
  wsUrl: string,
  targetInfo: CdpTargetInfo,
  defaultTimeoutMs = 60_000,
  signal?: AbortSignal,
  logPath?: string,
): Promise<CdpSession> {
  return new Promise((resolveSession, rejectSession) => {
    if (signal?.aborted) {
      rejectSession(new Error('CDP session creation aborted'));
      return;
    }

    // Optional CDP event log: NDJSON written to logPath for terminal visibility.
    // Usage: tail -f ~/.octocode/chrome-debug/port-9222/cdp-events.jsonl
    function cdpLog(entry: Record<string, unknown>): void {
      if (!logPath) return;
      try { fs.appendFileSync(logPath, JSON.stringify(entry) + '\n'); } catch { /* non-critical */ }
    }

    const WS = globalThis.WebSocket as typeof WebSocket;
    if (!WS) {
      rejectSession(new Error('Native WebSocket not available — Node 22+ required'));
      return;
    }

    const ws = new WS(wsUrl);
    let msgId = 1;
    const pending = new Map<
      number,
      { res: (v: Record<string, unknown>) => void; rej: (e: Error) => void; timer: ReturnType<typeof setTimeout> }
    >();
    const handlers = new Map<string, Set<(p: Record<string, unknown>, m: { sessionId?: string }) => void>>();
    let isClosed = false;

    function drainPending(reason: string): void {
      if (pending.size === 0) return;
      const err = new Error(reason);
      pending.forEach(({ rej, timer }) => { clearTimeout(timer); rej(err); });
      pending.clear();
    }

    function onAbort(): void {
      drainPending('CDP session aborted');
      ws.close();
    }

    signal?.addEventListener('abort', onAbort, { once: true });

    ws.onopen = () => {
      const session: CdpSession = {
        targetInfo,
        get closed() { return isClosed; },

        send(method: string, params: Record<string, unknown> = {}, sessionId?: string, timeoutMs = defaultTimeoutMs): Promise<Record<string, unknown>> {
          if (isClosed) return Promise.reject(new Error('Session already closed'));
          if (signal?.aborted) return Promise.reject(new Error('Session aborted'));
          return new Promise((res, rej) => {
            const id = msgId++;
            const timer = setTimeout(() => {
              pending.delete(id);
              rej(new Error(`CDP timeout (${timeoutMs}ms) for: ${method}`));
            }, timeoutMs);
            pending.set(id, { res, rej, timer });
            const payload: Record<string, unknown> = { id, method, params };
            if (sessionId) payload['sessionId'] = sessionId;
            ws.send(JSON.stringify(payload));
            cdpLog({ dir: '→', id, method, params: Object.keys(params), ts: Date.now() });
          });
        },

        on(event: string, handler: (p: Record<string, unknown>, m: { sessionId?: string }) => void): void {
          if (!handlers.has(event)) handlers.set(event, new Set());
          handlers.get(event)!.add(handler);
        },

        off(event: string, handler: (p: Record<string, unknown>, m: { sessionId?: string }) => void): void {
          handlers.get(event)?.delete(handler);
        },

        close(): void {
          if (isClosed) return;
          isClosed = true;
          signal?.removeEventListener('abort', onAbort);
          drainPending('Session closed');
          handlers.clear();
          try { ws.close(); } catch { /* ignore */ }
        },
      };

      resolveSession(session);
    };

    ws.onmessage = (evt: MessageEvent) => {
      let msg: Record<string, unknown>;
      try {
        msg = JSON.parse(typeof evt === 'string' ? evt : evt.data) as Record<string, unknown>;
      } catch {
        return;
      }

      const msgId = msg['id'] as number | undefined;
      if (msgId !== undefined && pending.has(msgId)) {
        const { res, rej, timer } = pending.get(msgId)!;
        pending.delete(msgId);
        clearTimeout(timer);
        const err = msg['error'] as { code?: number; message?: string } | undefined;
        if (err) {
          rej(new Error(`CDP error [${err.code ?? 0}]: ${err.message ?? 'unknown'}`));
        } else {
          res((msg['result'] as Record<string, unknown>) ?? {});
        }
      } else if (msg['method']) {
        const event = msg['method'] as string;
        const params = (msg['params'] as Record<string, unknown>) ?? {};
        const meta = msg['sessionId'] ? { sessionId: msg['sessionId'] as string } : {};
        handlers.get(event)?.forEach((h) => {
          try { h(params, meta); } catch { /* handler errors don't crash session */ }
        });
        handlers.get('*')?.forEach((h) => {
          try { h(params, meta); } catch { /* ignore */ }
        });
        cdpLog({ dir: '←', event, ts: Date.now() });
      }
    };

    ws.onerror = (e: Event) => {
      const msg = (e as ErrorEvent)?.message ?? String(e);
      drainPending(`WebSocket error: ${msg}`);
      if (!isClosed) rejectSession(new Error(`WebSocket error: ${msg}`));
    };

    ws.onclose = () => {
      drainPending('WebSocket closed unexpectedly');
    };
  });
}

// ─── Target selection ─────────────────────────────────────────────────────────

export async function selectTarget(
  port: number,
  opts: {
    newTab?: string;
    targetId?: string;
    targetUrl?: string;
    targetType?: string;
  },
): Promise<{ wsUrl: string; targetInfo: CdpTargetInfo; openedTabId?: string; via: string }> {
  const { newTab, targetId, targetUrl, targetType } = opts;

  if (newTab) {
    const tab = await openTab(port, newTab);
    // Brief wait for navigation
    await new Promise((r) => setTimeout(r, 800));
    if (!tab.webSocketDebuggerUrl) throw new Error(`No WebSocket URL for new tab at ${newTab}`);
    return {
      wsUrl: tab.webSocketDebuggerUrl,
      targetInfo: tab,
      openedTabId: tab.id,
      via: 'new-tab',
    };
  }

  const targets = await getTargets(port);

  if (targetId) {
    const t = targets.find((x) => x.id === targetId);
    if (!t) throw new Error(`Target ${targetId} not found`);
    if (!t.webSocketDebuggerUrl) throw new Error(`No WebSocket URL for target ${targetId}`);
    await activateTarget(port, targetId);
    return { wsUrl: t.webSocketDebuggerUrl, targetInfo: t, via: 'target-id' };
  }

  if (targetUrl) {
    const pool = targetType ? targets.filter((t) => t.type === targetType) : targets;
    const t = pool.find((x) => x.url && x.url.includes(targetUrl));
    if (!t) {
      const available = targets.map((x) => `  [${x.type}] ${x.url}`).join('\n');
      throw new Error(`No target matching URL "${targetUrl}".\nAvailable:\n${available}`);
    }
    if (!t.webSocketDebuggerUrl) throw new Error(`No WebSocket URL for target ${t.id}`);
    return { wsUrl: t.webSocketDebuggerUrl, targetInfo: t, via: 'target-url' };
  }

  if (targetType) {
    const t = targets.find((x) => x.type === targetType);
    if (!t) {
      const types = [...new Set(targets.map((x) => x.type))].join(', ');
      throw new Error(`No target of type "${targetType}". Available: ${types}`);
    }
    if (!t.webSocketDebuggerUrl) throw new Error(`No WebSocket URL for target type ${targetType}`);
    return { wsUrl: t.webSocketDebuggerUrl, targetInfo: t, via: 'target-type' };
  }

  // Default: first page
  const pages = targets.filter((t) => t.type === 'page');
  if (pages.length === 0) {
    throw new Error('No page targets. Open a tab in Chrome or pass newTab with a URL.');
  }
  const t = pages[0]!;
  if (!t.webSocketDebuggerUrl) throw new Error('No WebSocket URL for first page target');
  return { wsUrl: t.webSocketDebuggerUrl, targetInfo: t, via: 'first-page' };
}

// ─── Chrome 136 detection + launch ───────────────────────────────────────────

function parseChromeVersion(browserString: string): number {
  // "Chrome/137.0.7151.55" → 137
  const m = browserString.match(/Chrome\/(\d+)/i);
  return m ? parseInt(m[1]!, 10) : 0;
}

export function findChromePath(): string {
  const platform = process.platform;
  const candidates =
    platform === 'darwin'
      ? [
          '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
          '/Applications/Chromium.app/Contents/MacOS/Chromium',
        ]
      : platform === 'win32'
      ? [
          'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
          'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
        ]
      : [
          '/usr/bin/google-chrome',
          '/usr/bin/chromium',
          '/usr/bin/chromium-browser',
          '/snap/bin/chromium',
        ];

  for (const p of candidates) {
    if (fs.existsSync(p)) return p;
  }

  // Try PATH
  try {
    const which = execSync('which google-chrome chromium 2>/dev/null || true', { encoding: 'utf8' });
    const found = which.trim().split('\n').filter(Boolean)[0];
    if (found) return found;
  } catch { /* ignore */ }

  throw new Error(
    'Chrome not found. Install Google Chrome or Chromium, or start Chrome manually with ' +
    '--remote-debugging-port=9222 --user-data-dir=~/.octocode/chrome-debug/profile',
  );
}

export function getDefaultToolUserDataDir(): string {
  return path.join(os.homedir(), '.octocode', 'chrome-debug', 'profile');
}

/** Port-specific profile so parallel Chrome instances never share data. */
export function getPortUserDataDir(port: number): string {
  return path.join(os.homedir(), '.octocode', 'chrome-debug', `profile-${port}`);
}

export async function launchChrome(opts: {
  port: number;
  userDataDir: string;
  headless?: boolean;
  url?: string;
}): Promise<{ pid: number | undefined }> {
  const chromePath = findChromePath();
  const { port, userDataDir, headless = false, url } = opts;

  fs.mkdirSync(userDataDir, { recursive: true });

  const args = [
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${userDataDir}`,
    // Automation essentials
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-background-mode',
    '--disable-background-networking',
    '--disable-client-side-phishing-detection',
    '--disable-component-extensions-with-background-pages',
    '--disable-default-apps',
    '--disable-extensions',
    '--disable-hang-monitor',
    '--disable-popup-blocking',
    '--disable-prompt-on-repost',
    '--disable-sync',
    '--disable-translate',
    '--metrics-recording-only',
    '--no-startup-window',
    '--password-store=basic',
    '--safebrowsing-disable-auto-update',
    // macOS: suppress system keychain + update dialogs
    ...(process.platform === 'darwin' ? ['--use-mock-keychain'] : []),
    // Disable features that interfere with automation
    '--disable-features=TranslateUI,MediaRouter,OptimizationHints',
  ];

  if (headless) {
    args.push(
      '--headless=new',
      '--disable-gpu',
      '--disable-dev-shm-usage',
      '--hide-scrollbars',
      '--mute-audio',
    );
    if (process.platform === 'linux') {
      args.push('--no-sandbox', '--disable-setuid-sandbox');
    }
  }

  if (url) args.push(url);

  const child = spawn(chromePath, args, { detached: true, stdio: 'ignore' });
  child.unref();

  return { pid: child.pid };
}

// ─── Session metadata ─────────────────────────────────────────────────────────

export function getSessionDir(workspaceCwd: string, port: number): string {
  return path.join(workspaceCwd, '.octocode', 'chrome-debug', `port-${port}`);
}

export function getScreenshotDir(workspaceCwd?: string): string {
  if (workspaceCwd) {
    return path.join(workspaceCwd, '.octocode', 'screenshots');
  }
  return path.join(getOctocodeHome(), 'screenshots');
}

export function readSessionMeta(sessionFile: string): SessionMetadata | null {
  try {
    return JSON.parse(fs.readFileSync(sessionFile, 'utf8')) as SessionMetadata;
  } catch {
    return null;
  }
}

export function writeSessionMeta(sessionFile: string, meta: SessionMetadata): void {
  fs.mkdirSync(path.dirname(sessionFile), { recursive: true });
  fs.writeFileSync(sessionFile, JSON.stringify(meta, null, 2) + '\n', { mode: 0o600 });
}

// ─── Infer identity ───────────────────────────────────────────────────────────

export async function inferIdentity(
  session: CdpSession,
  mode: 'attached' | 'launched',
  version: CdpVersionInfo,
  userDataDir?: string,
): Promise<SessionIdentity> {
  const identity: SessionIdentity = {
    mode,
    userDataDir,
    browser: version.Browser ?? 'unknown',
  };

  try {
    // Get user agent from active tab
    const uaResult = await session.send('Runtime.evaluate', {
      expression: 'JSON.stringify({ ua: navigator.userAgent, host: location.host, path: location.pathname })',
      returnByValue: true,
    });
    const pageInfo = JSON.parse((uaResult['result'] as Record<string, unknown>)?.['value'] as string ?? '{}') as {
      ua?: string; host?: string; path?: string;
    };
    identity.userAgent = pageInfo.ua;
    identity.tabHost = pageInfo.host;
    identity.tabPath = pageInfo.path;
  } catch { /* page may not be ready */ }

  try {
    // Get cookie names only (never values)
    const cookieResult = await session.send('Network.getCookies', {});
    const cookies = (cookieResult['cookies'] as Array<{ name?: string }>) ?? [];
    identity.cookieNames = cookies
      .map((c) => c.name)
      .filter((n): n is string => typeof n === 'string')
      .slice(0, 20); // cap at 20 names
  } catch { /* Network domain may not be enabled */ }

  return identity;
}

// ─── Secret redaction ─────────────────────────────────────────────────────────

const REDACT_PATTERNS: Array<RegExp> = [
  // Bearer tokens
  /Bearer\s+[A-Za-z0-9._~+/-]{20,}/gi,
  // JWT
  /eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g,
  // Generic long base64 (>40 chars of [A-Za-z0-9+/=])
  /\b[A-Za-z0-9+/]{40,}={0,2}\b/g,
  // Cookie value patterns (name=value pairs where name suggests auth)
  /(?:token|auth|jwt|secret|password|apikey|api_key|credential|sessionid|sid|sess)=[^\s;&"']+/gi,
];

export function redactEvidence(text: string): string {
  let result = text;
  for (const pattern of REDACT_PATTERNS) {
    result = result.replace(pattern, '<redacted>');
  }
  return result;
}

export function redactObject(obj: unknown): unknown {
  if (typeof obj === 'string') return redactEvidence(obj);
  if (Array.isArray(obj)) return obj.map(redactObject);
  if (obj !== null && typeof obj === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      // Redact values of sensitive keys entirely. Includes value-carrying keys
      // returned by raw CDP calls (Network.getCookies → cookieValue, getResponseBody
      // → body) so short/plain secrets that pattern-based string redaction misses
      // don't leak into details.
      if (/token|auth|jwt|secret|password|apikey|api_key|credential|sessionid|sid|cookie_value|cookievalue|responsebody|response_body|\bbody\b/i.test(k)) {
        out[k] = '<redacted>';
      } else {
        out[k] = redactObject(v);
      }
    }
    return out;
  }
  return obj;
}

// ─── Screenshot writer ────────────────────────────────────────────────────────

export function buildScreenshotFilename(
  scheme: string,
  urlOrSlug: string | undefined,
  ext: string,
): string {
  const now = new Date();
  const Y = now.getUTCFullYear();
  const M = String(now.getUTCMonth() + 1).padStart(2, '0');
  const D = String(now.getUTCDate()).padStart(2, '0');
  const h = String(now.getUTCHours()).padStart(2, '0');
  const m = String(now.getUTCMinutes()).padStart(2, '0');
  const s = String(now.getUTCSeconds()).padStart(2, '0');
  const ts = `${Y}${M}${D}-${h}${m}${s}`;
  const slug = urlOrSlug
    ? urlOrSlug
        .replace(/^https?:\/\//, '')
        .replace(/[^\w-]/g, '-')
        .slice(0, 40)
        .replace(/-+$/, '')
    : 'capture';
  return `${ts}-${scheme}-${slug}.${ext}`;
}

export interface ScreenshotResult {
  path: string;
  evidenceLine: string;
}

export async function captureScreenshot(
  session: CdpSession,
  opts: {
    screenshotDir: string;
    scheme: string;
    format?: 'png' | 'jpeg' | 'webp' | 'pdf';
    quality?: number;
    clip?: { x: number; y: number; width: number; height: number; scale?: number };
    fullPage?: boolean;
    targetUrl?: string;
  },
): Promise<ScreenshotResult> {
  const { screenshotDir, scheme, format = 'png', quality, clip, fullPage, targetUrl } = opts;

  fs.mkdirSync(screenshotDir, { recursive: true });

  const filename = buildScreenshotFilename(scheme, targetUrl, format === 'pdf' ? 'pdf' : format);
  const filepath = path.join(screenshotDir, filename);

  if (format === 'pdf') {
    const result = await session.send('Page.printToPDF', { printBackground: true });
    const data = result['data'] as string;
    fs.writeFileSync(filepath, Buffer.from(data, 'base64'));
    return { path: filepath, evidenceLine: `[SCREENSHOT] ${filepath}` };
  }

  // Screenshot
  const ssParams: Record<string, unknown> = { format };
  if (quality !== undefined && format !== 'png') ssParams['quality'] = quality;
  if (clip) ssParams['clip'] = clip;

  if (fullPage) {
    ssParams['captureBeyondViewport'] = true;
    // Fallback: get layout metrics to compute full-page dimensions
    try {
      const metrics = await session.send('Page.getLayoutMetrics', {});
      const contentSize = metrics['contentSize'] as { width?: number; height?: number } | undefined;
      if (contentSize?.width && contentSize.height) {
        await session.send('Emulation.setDeviceMetricsOverride', {
          width: Math.ceil(contentSize.width),
          height: Math.ceil(contentSize.height),
          deviceScaleFactor: 1,
          mobile: false,
        }).catch(() => undefined);
      }
    } catch { /* older Chrome without getLayoutMetrics */ }
  }

  const result = await session.send('Page.captureScreenshot', ssParams);
  const data = result['data'] as string;
  fs.writeFileSync(filepath, Buffer.from(data, 'base64'));

  // Reset device metrics if we overrode them
  if (fullPage) {
    await session.send('Emulation.clearDeviceMetricsOverride', {}).catch(() => undefined);
  }

  return { path: filepath, evidenceLine: `[SCREENSHOT] ${filepath}` };
}

// ─── CDP retry marker ─────────────────────────────────────────────────────────

export function buildRetryMarker(error: Error, method: string): string {
  return `[CDP_RETRY_NEEDED] method=${method} error="${error.message}"\n` +
    `[CDP_RETRY_NEEDED] Fix: ensure the domain for "${method}" is enabled before calling it, check parameter names, and re-run.`;
}

export function isCdpError(error: Error): boolean {
  return /CDP error \[|CDP timeout/.test(error.message);
}

// ─── Main connect function ────────────────────────────────────────────────────

/**
 * Connect to Chrome via CDP. Supports attach (to already-running) and launch modes.
 *
 * Chrome 136+ constraint: when launching, always supply a non-default --user-data-dir.
 * Real-user path is attach-first: user starts Chrome with --remote-debugging-port themselves.
 */
export async function connectToChrome(opts: ChromeDebugConnectOptions): Promise<ChromeConnection> {
  const {
    port = 9222,
    targetId,
    targetUrl,
    targetType,
    newTab,
    launch = false,
    headless = false,
    timeoutMs = 60_000,
    signal,
    workspaceCwd,
  } = opts;

  // When launching, use a port-specific profile so parallel instances don't conflict.
  // When attaching, the profile is irrelevant (Chrome is already running).
  let userDataDir = opts.userDataDir ?? getPortUserDataDir(port);
  let mode: 'attached' | 'launched' = 'attached';

  // ── Step 1: Try to connect to already-running Chrome ─────────────────────
  let version: CdpVersionInfo | null = null;
  try {
    version = await getVersion(port);
  } catch {
    // Chrome not yet running on this port
  }

  // ── Step 2: If not running and launch requested, start Chrome ─────────────
  let launchedPid: number | undefined;
  if (!version && launch) {
    // Chrome 136+ requires non-default user-data-dir — we always supply one
    ({ pid: launchedPid } = await launchChrome({ port, userDataDir, headless, url: newTab }));
    mode = 'launched';

    // Wait up to 20s for Chrome to start
    let attempts = 0;
    while (attempts < 40) {
      await new Promise((r) => setTimeout(r, 500));
      try {
        version = await getVersion(port);
        break;
      } catch { /* still starting */ }
      attempts++;
    }
    if (!version) throw new Error(`Chrome did not start within 20s on port ${port}`);
  } else if (!version) {
    throw new Error(
      `Chrome not responding on port ${port}. ` +
      `Start Chrome with: --remote-debugging-port=${port} --user-data-dir=${userDataDir}\n` +
      `Or pass launch:true to let the tool start Chrome for you.\n` +
      `Note: Chrome ≥136 requires a non-default --user-data-dir for remote debugging.`,
    );
  }

  // ── Step 3: Check Chrome version for 136+ warning ────────────────────────
  const chromeMajor = parseChromeVersion(version.Browser ?? '');
  if (chromeMajor >= 136 && mode === 'launched') {
    // We always supply a non-default userDataDir so this should be fine.
    // Just verify we're not accidentally using the OS default.
    const defaultDirs = [
      path.join(os.homedir(), 'Library', 'Application Support', 'Google', 'Chrome'),
      path.join(os.homedir(), '.config', 'google-chrome'),
      path.join(process.env['LOCALAPPDATA'] ?? '', 'Google', 'Chrome', 'User Data'),
    ];
    if (defaultDirs.includes(path.resolve(userDataDir))) {
      throw new Error(
        `Chrome ≥${chromeMajor} blocks remote debugging with the OS default profile.\n` +
        `Use a non-default --user-data-dir. The tool default is: ${getDefaultToolUserDataDir()}\n` +
        `Or use attach mode: start Chrome yourself with --remote-debugging-port=${port} --user-data-dir=<custom>`,
      );
    }
  }

  // ── Step 4: Select target ─────────────────────────────────────────────────
  const { wsUrl, targetInfo, openedTabId, via } = await selectTarget(port, {
    newTab,
    targetId,
    targetUrl,
    targetType,
  });

  // ── Step 5: Open WebSocket session ────────────────────────────────────────
  // CDP event log — write NDJSON for terminal visibility: tail -f <logPath>
  const cdpLogPath = process.env['OCTOCODE_CDP_DEBUG'] === '1'
    ? path.join(
        getSessionDir(workspaceCwd ?? path.join(os.homedir(), '.octocode'), port),
        'cdp-events.jsonl',
      )
    : undefined;
  if (cdpLogPath) {
    fs.mkdirSync(path.dirname(cdpLogPath), { recursive: true });
    fs.writeFileSync(cdpLogPath, ''); // truncate on new session
  }
  const session = await createCdpSession(wsUrl, targetInfo, timeoutMs, signal, cdpLogPath);

  // ── Step 6: Infer identity + session metadata ─────────────────────────────
  const identity = await inferIdentity(session, mode, version, mode === 'launched' ? userDataDir : undefined);

  const sessionDir = getSessionDir(workspaceCwd ?? process.cwd(), port);
  const sessionFile = path.join(sessionDir, 'session.json');
  const screenshotDir = getScreenshotDir(workspaceCwd);

  const metadata: SessionMetadata = {
    port,
    browser: version.Browser ?? 'unknown',
    mode,
    userDataDir: mode === 'launched' ? userDataDir : undefined,
    lastConnectedAt: new Date().toISOString(),
    activeTarget: {
      id: targetInfo.id,
      type: targetInfo.type,
      url: targetInfo.url,
      title: targetInfo.title,
      via,
    },
    identity,
  };

  writeSessionMeta(sessionFile, metadata);

  // ── Step 7: Store opened tab ID for cleanup ───────────────────────────────
  const sessionMeta = session as unknown as Record<string, unknown>;
  if (openedTabId) {
    sessionMeta['_openedTabId'] = openedTabId;
  }
  sessionMeta['_port'] = port;
  // Record the pid of a Chrome we launched so cleanup can terminate it (else it
  // orphans, since Chrome is spawned detached+unref). Attach-mode leaves this unset.
  if (launchedPid !== undefined) {
    sessionMeta['_launchedPid'] = launchedPid;
  }

  return { session, version, metadata, screenshotDir, sessionFile };
}

// ─── Cleanup ──────────────────────────────────────────────────────────────────

export async function cleanupConnection(
  session: CdpSession,
  keepTab: boolean,
  killLaunched = false,
): Promise<void> {
  const s = session as unknown as Record<string, unknown>;
  const openedTabId = s['_openedTabId'] as string | undefined;
  const port = s['_port'] as number | undefined;
  const launchedPid = s['_launchedPid'] as number | undefined;

  if (!keepTab && openedTabId && port) {
    await closeTab(port, openedTabId).catch(() => undefined);
  }

  session.close();

  // Only terminate Chrome when this connection launched it AND the caller asked
  // for full cleanup — attach-mode connections must never kill the user's browser.
  if (killLaunched && launchedPid !== undefined) {
    try {
      process.kill(launchedPid, 'SIGTERM');
    } catch {
      // Already gone or not killable — nothing to do.
    }
    s['_launchedPid'] = undefined;
  }
}
