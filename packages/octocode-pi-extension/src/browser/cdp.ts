import fs from 'node:fs';
import path from 'node:path';
import { launch, type LaunchedChrome } from 'chrome-launcher';
import { BROWSER_VISIBLE_ENV, envFlag, envInt } from '../shared/env.js';
import { HOME_NAMES, outputDir, sweepDeadOutputs } from '../shared/home.js';
import { processAlive } from '../shared/process.js';
import { withTimeout } from '../shared/util.js';
import { applyDownloads, applyLocale } from './emulation.js';

/** The error an aborted browser operation rejects with: the signal's own reason when it is an Error. */
export const abortError = (signal: AbortSignal): Error => (signal.reason instanceof Error ? signal.reason : new Error('Aborted'));

/**
 * Minimal Chrome DevTools Protocol session over the page websocket. Launches its own Chrome, or, when the user opts in
 * with OCTOCODE_CHROME_PORT, works in a tab of the Chrome listening there (to reuse its logged-in profile).
 */
class CdpPage {
  readonly #socket: WebSocket;
  readonly #pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  readonly #listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
  #nextId = 1;

  private constructor(socket: WebSocket) {
    this.#socket = socket;
    socket.addEventListener('message', (event) => {
      // A throwing EventTarget listener surfaces as an uncaught exception in Pi's process.
      let message: { id?: number; result?: unknown; error?: { message: string }; method?: string; params?: Record<string, unknown> };
      try {
        message = JSON.parse(String(event.data)) as typeof message;
      } catch {
        return;
      }
      if (message.id !== undefined) {
        const pending = this.#pending.get(message.id);
        this.#pending.delete(message.id);
        if (message.error) pending?.reject(new Error(message.error.message));
        else pending?.resolve(message.result);
      } else if (message.method) {
        for (const listener of this.#listeners) {
          try {
            listener(message.method, message.params ?? {});
          } catch {
            // One bad event must not break the session.
          }
        }
      }
    });
    socket.addEventListener('close', () => {
      for (const pending of this.#pending.values()) pending.reject(new Error('Browser connection closed'));
      this.#pending.clear();
    });
  }

  static async connect(url: string): Promise<CdpPage> {
    const socket = new WebSocket(url);
    try {
      await withTimeout(
        new Promise<void>((resolve, reject) => {
          socket.addEventListener('open', () => resolve(), { once: true });
          socket.addEventListener('error', () => reject(new Error(`Cannot connect to ${url}`)), { once: true });
        }),
        10_000,
        'Browser connect',
      );
    } catch (error) {
      // A timed-out socket would otherwise keep connecting in the background.
      socket.close();
      throw error;
    }
    return new CdpPage(socket);
  }

  get open(): boolean {
    return this.#socket.readyState === WebSocket.OPEN;
  }

  /** Send a CDP command; `signal` rejects the wait (the command may still run in the page). */
  send<T = Record<string, unknown>>(method: string, params: Record<string, unknown> = {}, signal?: AbortSignal): Promise<T> {
    const id = this.#nextId++;
    if (!this.open) return Promise.reject(new Error('Browser connection closed'));
    if (signal?.aborted) return Promise.reject(abortError(signal));
    let onAbort: (() => void) | undefined;
    const result = new Promise<T>((resolve, reject) => {
      this.#pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
      if (signal) signal.addEventListener('abort', (onAbort = () => reject(abortError(signal))), { once: true });
    });
    this.#socket.send(JSON.stringify({ id, method, params }));
    return withTimeout(result, 30_000, method).finally(() => {
      this.#pending.delete(id);
      if (onAbort) signal?.removeEventListener('abort', onAbort);
    });
  }

  on(listener: (method: string, params: Record<string, unknown>) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /** Resolves on the first `method` event or after `timeoutMs`; rejects when `signal` aborts. */
  waitFor(method: string, timeoutMs: number, signal?: AbortSignal): Promise<void> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) return reject(abortError(signal));
      const timer = setTimeout(done, timeoutMs);
      const off = this.on((name) => name === method && done());
      const onAbort = () => {
        cleanup();
        reject(abortError(signal!));
      };
      signal?.addEventListener('abort', onAbort, { once: true });
      function cleanup() {
        clearTimeout(timer);
        off();
        signal?.removeEventListener('abort', onAbort);
      }
      function done() {
        cleanup();
        resolve();
      }
    });
  }

  /** Resolves `work`, or `undefined` as soon as a JavaScript dialog opens (the page then blocks until it is answered). */
  async untilDialog<T>(work: Promise<T>): Promise<T | undefined> {
    work.catch(() => undefined);
    let off: (() => void) | undefined;
    const opened = new Promise<undefined>((resolve) => {
      off = this.on((method) => method === 'Page.javascriptDialogOpening' && resolve(undefined));
    });
    try {
      return await Promise.race([work, opened]);
    } finally {
      off?.();
    }
  }

  /** Evaluate an expression in the page and return its JSON value. */
  async evaluate<T = unknown>(expression: string, signal?: AbortSignal): Promise<T> {
    const response = await this.send<{ result?: { value?: T }; exceptionDetails?: { text?: string; exception?: { description?: string } } }>('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    }, signal);
    if (response.exceptionDetails) throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text ?? 'Evaluation failed');
    return response.result?.value as T;
  }

  close(): void {
    this.#socket.close();
  }
}

export interface BrowserSession {
  page: CdpPage;
  /** DevTools target id of the page (tab) this session drives now. */
  targetId: string;
  /** DevTools port of the browser this session drives. */
  port: number;
  /** `own`: Chrome this session launched (headless unless OCTOCODE_BROWSER_HEADLESS=0); `shared`: a Chrome already running, driven in a tab of our own; `attached`: someone else's window, driven in place. */
  kind: 'own' | 'shared' | 'attached';
  headless?: boolean;
  chrome?: LaunchedChrome;
  /** Profile folder that outlives the session (visible mode): the user's logins stay, and Chrome stays open at shutdown. */
  profile?: string;
  /** The JavaScript dialog (alert, confirm, prompt, beforeunload) waiting for an answer. */
  dialog?: { type: string; message: string };
  /** Downloads by guid, once `download` turned them on. */
  downloads: Map<string, Download>;
  downloadDir?: string;
  /** Closes the tab this session opened in a Chrome it attached to. */
  closeTab?: () => Promise<void>;
  console: string[];
}

export interface Download {
  name: string;
  state: 'inProgress' | 'completed' | 'canceled';
  bytes: number;
}

export interface PageTarget {
  id: string;
  webSocketDebuggerUrl: string;
  title?: string;
  url?: string;
}

interface OpenOptions {
  /** DevTools port to attach to instead of OCTOCODE_CHROME_PORT. */
  port?: number;
  /** Drive the browser's existing page instead of opening a tab (a browser someone else is showing in a window). */
  reuseTab?: boolean;
}

/** The persistent profile visible mode uses: `<Octocode home>/pi-browser-profile`. */
const persistentProfileDir = (env: NodeJS.ProcessEnv): string => outputDir(HOME_NAMES.browserProfile, env);

/**
 * A launched Chrome's profile folder: `profile` (kept) or a fresh one under `<Octocode home>/pi-browser/` (not the
 * temp dir) that is removed when it closes.
 */
async function launchChrome(headless: boolean, profile?: string): Promise<LaunchedChrome> {
  // Headless Chrome defaults to an 800x600 window, where many sites collapse into a mobile layout (no search box).
  const flags = [...(headless ? ['--headless=new', '--window-size=1366,900'] : []), '--no-first-run', '--no-default-browser-check'];
  if (profile) return launch({ startingUrl: 'about:blank', userDataDir: profile, chromeFlags: flags });
  const root = outputDir(HOME_NAMES.browser);
  sweepDeadOutputs(root, 0);
  const userDataDir = fs.mkdtempSync(path.join(root, `${process.pid}-`));
  let chrome: LaunchedChrome;
  try {
    chrome = await launch({ startingUrl: 'about:blank', userDataDir, chromeFlags: flags });
  } catch (error) {
    fs.rmSync(userDataDir, { recursive: true, force: true });
    throw error;
  }
  const kill = chrome.kill.bind(chrome);
  chrome.kill = async () => {
    try {
      await kill();
    } finally {
      fs.rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5 });
    }
  };
  return chrome;
}

/** The DevTools port of a Chrome already running on `profile`, from the DevToolsActivePort file Chrome writes there. */
async function profilePort(profile: string): Promise<number | undefined> {
  let port: number;
  try {
    port = Number(fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]);
  } catch {
    return undefined;
  }
  if (!Number.isInteger(port) || port <= 0) return undefined;
  const ok = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1_000) }).then((r) => r.ok, () => false);
  return ok ? port : undefined;
}

/** Chrome's SingletonLock (a `<host>-<pid>` symlink) names a live process: another Chrome owns the profile. */
function profileLocked(profile: string): boolean {
  try {
    const pid = Number(fs.readlinkSync(path.join(profile, 'SingletonLock')).split('-').pop());
    return Number.isInteger(pid) && pid > 0 && processAlive(pid);
  } catch {
    return false;
  }
}

export async function openBrowser(env: NodeJS.ProcessEnv = process.env, options: OpenOptions = {}): Promise<BrowserSession> {
  // Someone else's Chrome only on an explicit port: an `attach` call, or the user's OCTOCODE_CHROME_PORT opt-in.
  const shared = options.port ?? sharedChromePort(env);
  let chrome: LaunchedChrome | undefined;
  let headless: boolean | undefined;
  let closeTab: (() => Promise<void>) | undefined;
  let onProfile = false;
  const profile = !options.port && envFlag(env, BROWSER_VISIBLE_ENV) ? persistentProfileDir(env) : undefined;
  // Attaching to the user's Chrome: work in a tab of our own instead of taking over one they have open.
  let port = shared ?? 0;
  let target = shared === undefined ? undefined : await (options.reuseTab ? firstPage(shared) : newTab(shared)).catch(() => undefined);
  if (options.reuseTab && !target) throw new Error(`No browser is listening on DevTools port ${port}.`);
  if (!target && profile) {
    // Visible mode: the persistent profile's Chrome may still be open from an earlier session (a login hand-off).
    const running = await profilePort(profile);
    if (running) {
      target = await newTab(running);
      port = running;
      onProfile = true;
    } else if (profileLocked(profile)) {
      throw new Error(`The browser profile ${profile} is open in a Chrome without DevTools. Close that Chrome and retry.`);
    }
  }
  if (target && !options.reuseTab) {
    const id = target.id;
    const tabPort = port;
    closeTab = async () => {
      await fetch(`http://127.0.0.1:${tabPort}/json/close/${id}`, { signal: AbortSignal.timeout(2_000) });
    };
  } else if (!target) {
    headless = !profile && envFlag(env, 'OCTOCODE_BROWSER_HEADLESS', true);
    chrome = await launchChrome(headless, profile);
    onProfile = profile !== undefined;
    // Chrome writes this file only for port 0; chrome-launcher picks a fixed port, so record it for the next run.
    if (profile) fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${chrome.port}\n`);
    try {
      target = await firstPage(chrome.port);
    } catch (error) {
      await chrome.kill();
      throw error;
    }
  }
  let page: CdpPage | undefined;
  try {
    page = await CdpPage.connect(target.webSocketDebuggerUrl);
    const kind = chrome ? 'own' : options.reuseTab ? 'attached' : 'shared';
    const session = startSession(page, target.id, { port: chrome?.port ?? port, kind, chrome, headless, closeTab, profile: onProfile ? profile : undefined });
    await Promise.all([page.send('Page.enable'), page.send('Runtime.enable')]);
    await applyLocale(page, env);
    return session;
  } catch (error) {
    // Nothing owns the tab or the Chrome yet: release them instead of leaking a process or a tab.
    page?.close();
    await closeTab?.().catch(() => undefined);
    await chrome?.kill();
    throw error;
  }
}

/** `OCTOCODE_CHROME_PORT` (a TCP port, clamped to 1–65535) when set: the user's opt-in to share an open Chrome. */
export function sharedChromePort(env: NodeJS.ProcessEnv): number | undefined {
  return env['OCTOCODE_CHROME_PORT']?.trim() ? envInt(env, 'OCTOCODE_CHROME_PORT', 9222, { min: 1, max: 65_535 }) : undefined;
}

type SessionParts = Pick<BrowserSession, 'port' | 'kind'> & { chrome?: LaunchedChrome | undefined; headless?: boolean | undefined; closeTab?: (() => Promise<void>) | undefined; profile?: string | undefined };

function startSession(page: CdpPage, targetId: string, parts: SessionParts): BrowserSession {
  const session: BrowserSession = { page, targetId, port: parts.port, kind: parts.kind, console: [], downloads: new Map() };
  if (parts.chrome) session.chrome = parts.chrome;
  if (parts.headless !== undefined) session.headless = parts.headless;
  if (parts.closeTab) session.closeTab = parts.closeTab;
  if (parts.profile) session.profile = parts.profile;
  listen(session, page);
  return session;
}

/** Tracks the page's dialogs, downloads and console output on the session. */
function listen(session: BrowserSession, page: CdpPage): void {
  page.on((method, params) => {
    if (method === 'Page.javascriptDialogOpening') {
      session.dialog = { type: String(params['type']), message: String(params['message'] ?? '') };
    } else if (method === 'Page.javascriptDialogClosed') {
      delete session.dialog;
    } else if (method === 'Browser.downloadWillBegin' || method === 'Page.downloadWillBegin') {
      session.downloads.set(String(params['guid']), { name: String(params['suggestedFilename'] ?? 'download'), state: 'inProgress', bytes: 0 });
    } else if (method === 'Browser.downloadProgress' || method === 'Page.downloadProgress') {
      const download = session.downloads.get(String(params['guid']));
      if (download) session.downloads.set(String(params['guid']), { ...download, state: params['state'] as Download['state'], bytes: Number(params['receivedBytes'] ?? download.bytes) });
    } else if (method === 'Runtime.consoleAPICalled') {
      const args = Array.isArray(params['args']) ? params['args'] : [];
      const text = args.map((arg) => String((arg as { value?: unknown; description?: string }).value ?? (arg as { description?: string }).description ?? '')).join(' ');
      session.console = [...session.console.slice(-199), `[${String(params['type'])}] ${text}`];
    } else if (method === 'Runtime.exceptionThrown') {
      const details = params['exceptionDetails'] as { text?: string; exception?: { description?: string } } | undefined;
      session.console = [...session.console.slice(-199), `[exception] ${details?.exception?.description ?? details?.text ?? ''}`];
    }
  });
}

/** The browser's page targets (tabs), in the order Chrome lists them (most recently used first). */
export async function pageTargets(port: number): Promise<PageTarget[]> {
  const targets = (await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2_000) })).json()) as Array<Partial<PageTarget> & { type: string }>;
  return targets.filter((target): target is PageTarget & { type: string } => target.type === 'page' && typeof target.id === 'string' && typeof target.webSocketDebuggerUrl === 'string');
}

/** Opens a tab on `url` in the session's browser. */
export async function openTab(port: number, url: string): Promise<PageTarget> {
  const response = await fetch(`http://127.0.0.1:${port}/json/new?${encodeURI(url)}`, { method: 'PUT', signal: AbortSignal.timeout(5_000) });
  if (!response.ok) throw new Error(`Chrome on port ${port} refused a new tab (HTTP ${response.status})`);
  return (await response.json()) as PageTarget;
}

/**
 * Moves the session onto another tab: connects to it, applies the locale and (once `download` turned them on) the
 * download folder, starts tracking its dialogs, downloads and console, brings it to the front, and drops the old page
 * connection (the old tab stays open). The tab the session opened itself is still
 * the one `close` removes.
 */
export async function switchTo(session: BrowserSession, target: PageTarget): Promise<void> {
  const page = await CdpPage.connect(target.webSocketDebuggerUrl);
  try {
    await Promise.all([page.send('Page.enable'), page.send('Runtime.enable')]);
    await applyLocale(page, process.env);
    if (session.downloadDir) await applyDownloads(page, session.downloadDir);
  } catch (error) {
    page.close();
    throw error;
  }
  listen(session, page);
  const previous = session.page;
  session.page = page;
  session.targetId = target.id;
  session.console = [];
  delete session.dialog;
  previous.close();
  await page.send('Page.bringToFront').catch(() => undefined);
}

async function newTab(port: number): Promise<PageTarget> {
  const response = await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT', signal: AbortSignal.timeout(2_000) });
  if (!response.ok) throw new Error(`Chrome on port ${port} refused a new tab (HTTP ${response.status})`);
  return (await response.json()) as PageTarget;
}

/** The about:blank page of a Chrome this session launched. */
async function firstPage(port: number): Promise<PageTarget> {
  const targets = (await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(2_000) })).json()) as Array<Partial<PageTarget> & { type: string }>;
  const page = targets.find((target) => target.type === 'page' && target.webSocketDebuggerUrl && target.id);
  return page ? (page as PageTarget) : newTab(port);
}
